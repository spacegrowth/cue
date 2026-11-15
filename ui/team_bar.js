// ---------- the crew page's Needs you: one bar over the box, one ask at a time (team_page.js has the page) ----------
// Every ask on the page is one list, in one order: the lead's own first, then one-way ones, then the oldest. The bar
// shows one of them at a time, at one height whatever the count ("2 of 5 ‹ ›", "All 5 ›"), and never swaps the one it
// shows by itself: a new ask only raises the count; it changes when you answer, step, or pick a teammate. After any
// change its buttons are dead for a moment, so a second click can't land on the next ask; after an answer the cursor
// goes to the box. All N › (or the ask's words) puts every ask on the left, each whole with its buttons. While the
// lead itself asks, the box holds; ↪ puts a chip in the box, and the box's words become that answer.

/** How long the bar's buttons stay dead after the ask it shows changed. */
const CREW_BAR_GUARD_MS = 600;
/** Per lead, the ask the bar shows (its key) and where it was in the list, so the next one takes its place. */
const crewBarAt = new Map();
/** The ask the bar last drew, and when that changed: the guard runs from then. */
let crewBarDrawn = null, crewBarSince = 0;
/** The chip in the box: ↪ on an ask, its words go as that answer. { sid, id (the ask's item), label }. */
let crewChip = null;

/** The first line of what was said before an ask: its reason, as the ask's card says it. */
function crewAskWhy(it) {
  const c = (it.context || []).filter((x) => x.role !== "user").at(-1) || it.context?.at(-1);
  return c ? firstLine(c.text) : "";
}
/** An ask's buttons in the bar, from what it is: a permission ↪ Deny Allow; a question its choices (up to 3, and ↪
 *  for your own words), more than that Choose ›; anything else (a form, one answered in its terminal) Open ›, the
 *  Needs you page, where its card is whole. Allow and Deny carry the keys ⌘⇧A and ⌘⇧D take, unless it's one-way. */
function crewAskButtons(it, oneWay) {
  const id = esc(it.id), key = (k) => (oneWay ? "" : ` data-bar-key="${k}"`);
  const redir = (label) => `<button class="btn cpb-redir" data-cp-chip="${id}" aria-label="${label}">↪</button>`;
  if (it.kind === "permission" && !inTerminal(it)) {
    const plan = tool(it) === "exitplanmode";
    return `${redir("Tell it what to do instead")}<button class="btn deny" data-act="deny" data-id="${id}"${key("d")}>${plan ? "Keep planning" : "Deny"}</button><button class="btn primary" data-act="allow" data-id="${id}"${key("a")}>${plan ? "Approve plan" : "Allow"}</button>`;
  }
  const qs = it.kind === "question" ? questions(it) : [];
  if (qs.length === 1 && !qs[0].multiSelect && (qs[0].options || []).length <= 3) {
    return redir("Your own answer") + (qs[0].options || []).map((o, n) => `<button class="btn ${n === 0 ? "primary" : ""}" data-pick="0:${n}" data-id="${id}">${esc(o.label)}</button>`).join("");
  }
  return `<button class="btn" data-cp-all="1">${it.kind === "question" ? "Choose ›" : "Open ›"}</button>`;
}
/** A reviewed teammate waiting on you: the button its review calls for, the other beside it. */
function crewReadyButtons(sid, t) {
  if (mateReadyAct(t) === "fixes") return mateCommit(sid, t, "") + mateBtn(sid, t, "fixes", "Send fixes", "primary");
  return mateBtn(sid, t, "fixes", "Send fixes", "") + mateCommit(sid, t);
}

/** Every ask on lead `sid`'s page, in the bar's order, each { key, who, what, cmd, why, one_way, buttons, at } and
 *  what it came from (`it`, the ask's item; `mate`, a teammate; `html`, an add-on's row drawn as it gave it):
 *  the lead's own asks; each teammate's ask, or what it needs, or its review waiting for your Commit or Send fixes;
 *  then the add-on's (addOn.crewPageNeeds: items in this same shape, or with only `html`, an older add-on's). */
function crewAsks(sid) {
  const out = [], s = sessionOf(sid);
  if (!s) return out;
  const asks = state.items.filter((i) => i.session_id === sid && i.kind !== "waiting" && !answering.has(i.id));
  const ask = (it, who, lead) => ({ key: `a:${it.id}`, who, what: verb(it), cmd: summary(it), why: crewAskWhy(it), one_way: !!it.one_way, at: it.created_ms || 0, lead, it, buttons: crewAskButtons(it, !!it.one_way) });
  for (const it of asks.filter((i) => !mateOfAsk(i))) out.push(ask(it, "The lead", true));
  const handles = crewHandles(sid), groups = crewGroups(sid);
  for (const t of crewNeedRows(sid, groups, true).list) {
    const it = mateAsk(sid, t);
    if (it) { if (!answering.has(it.id)) out.push({ ...ask(it, t.name, false), mate: t }); continue; }
    const at = t.review_ms || t.ended_ms || t.started_ms || 0, k = `m:${mateKey(sid, t)}`;
    if (t.needs) out.push({ key: k, who: t.name, what: t.needs, cmd: "", why: "", one_way: false, at, mate: t, buttons: mateOneAct(sid, t, "") });
    else out.push({ key: k, who: t.name, what: `reviewed: ${crewFound(t)}`, cmd: "", why: handles ? "out of fix rounds: yours to settle" : crewFindingText((t.findings || [])[0] || ""), one_way: false, at, mate: t, buttons: crewReadyButtons(sid, t) });
  }
  const added = addOn.crewPageNeeds?.(sid) || { items: [], count: 0 };
  const theirs = added.items || (added.count && added.html ? [{ html: added.html }] : []);
  theirs.forEach((x, n) => {
    const shaped = x.what != null;
    out.push({ key: `x:${x.id ?? n}`, who: x.who || "", what: shaped ? x.what : "waits for you", cmd: x.cmd || "", why: x.why || "", one_way: !!x.one_way, at: x.at_ms || 0,
      html: x.html || "", buttons: shaped ? x.buttons || "" : `<button class="btn" data-cp-all="1">Open ›</button>` });
  });
  // The lead's own first, then one-way, then the oldest; the list keeps its order otherwise.
  return out.map((x, n) => [x, n]).sort(([a, i], [b, j]) => (!!b.lead - !!a.lead) || (!!b.one_way - !!a.one_way) || (a.at - b.at) || (i - j)).map(([x]) => x);
}
/** The ask the bar shows for lead `sid`: the one it showed while it's still there, else the one that took its place. */
function crewBarShown(sid, list = crewAsks(sid)) {
  if (!list.length) return null;
  const was = crewBarAt.get(sid), hit = was && list.findIndex((x) => x.key === was.key);
  const n = hit >= 0 ? hit : Math.min(was?.n || 0, list.length - 1);
  crewBarAt.set(sid, { key: list[n].key, n });
  return { x: list[n], n, list };
}
/** Show ask `key` in the bar (a teammate picked, Answer first). */
function crewBarShow(sid, key) {
  const list = crewAsks(sid), n = list.findIndex((x) => x.key === key);
  if (n >= 0) crewBarAt.set(sid, { key, n });
}
/** ‹ › and ⌘⇧↑ ⌘⇧↓: the ask before or after the one shown (round the ends). */
function crewBarStep(sid, by) {
  const cur = crewBarShown(sid);
  if (!cur || cur.list.length < 2) return;
  const n = (cur.n + by + cur.list.length) % cur.list.length;
  crewBarAt.set(sid, { key: cur.list[n].key, n });
  renderMain();
}
/** Whether the bar's buttons are dead now (the ask it shows changed a moment ago). */
const crewBarDead = () => Date.now() - crewBarSince < CREW_BAR_GUARD_MS;

/** The bar for lead `sid`: "" when nothing waits, or while the Needs you page is up (the page is it). */
function crewBarHtml(sid) {
  const cur = crewBarShown(sid);
  if (!cur || crewLeftOf(sid)?.kind === "needs") { crewBarDrawn = null; return ""; }
  const { x, n, list } = cur;
  // What it shows changed: its buttons are dead for a moment, then drawn live again.
  if (crewBarDrawn !== `${sid}|${x.key}`) {
    if (crewBarDrawn !== null) { crewBarSince = Date.now(); setTimeout(renderMain, CREW_BAR_GUARD_MS + 20); }
    crewBarDrawn = `${sid}|${x.key}`;
  }
  const many = list.length > 1;
  const steps = many ? `<span class="cpb-n">${n + 1} of ${list.length}</span><button class="cpb-step" data-cp-step="-1" aria-label="The ask before">‹</button><button class="cpb-step" data-cp-step="1" aria-label="The next ask">›</button><button class="mt-lk cpb-all" data-cp-all="1">All ${list.length} ›</button>` : "";
  const keyed = /data-bar-key/.test(x.buttons);
  const right = x.one_way ? `<span class="cpb-ow">one-way: answered with a click, never a key</span>`
    : keyed ? `<span class="cpb-keys"><kbd>⌘⇧A</kbd> ${x.it?.kind === "permission" ? "allow" : "yes"} · <kbd>⌘⇧D</kbd> ${x.it?.kind === "permission" ? "deny" : "no"}</span>` : "";
  const who = x.mate ? `<button class="cpb-who" data-cp-mate="${esc(mateKey(sid, x.mate))}">${esc(x.who)}</button>` : x.who ? `<b class="cpb-who">${esc(x.who)}</b>` : "";
  const words = `<button class="cpb-words" data-cp-all="1">${x.one_way ? `<span class="cpb-tag">one-way</span>` : ""}<span class="cpb-what">${esc(x.what)}</span>${x.cmd ? `<code class="cpb-cmd">${esc(x.cmd)}</code>` : ""}${x.why ? `<span class="cpb-why">${crewCode(x.why)}</span>` : ""}</button>`;
  return `<div class="cp-bar ${crewBarDead() ? "dead" : ""}" data-key="cp-bar" data-cp-bar="${esc(sid)}" role="region" aria-label="Needs you">
    <div class="cpb-1"><span class="cpb-lab">Needs you</span>${steps}<span class="grow"></span>${right}</div>
    <div class="cpb-2">${who}${words}<span class="cpb-btns">${x.buttons}</span></div></div>`;
}

/** The Needs you page on the left: every ask whole, in the bar's order, each with its buttons (an ask's own card,
 *  a teammate's row as the crew's tray has it, an add-on's row as it gave it). */
function crewNeedsPage(sid) {
  const list = crewAsks(sid);
  const row = (x) => {
    if (x.it && !x.mate) return `<div class="cpn-row act-card" tabindex="-1"><div class="act-head"><b>${esc(x.who)}</b> ${esc(x.what)}${x.one_way ? ` <span class="cpb-tag">one-way</span>` : ""}</div>${requestBody(x.it)}${x.it.kind !== "question" ? whyLine(x.it) : ""}${decisionButtons(x.it)}</div>`;
    if (x.mate) return `<div class="cpn-row" tabindex="-1">${mateNeeds(sid, x.mate) ? mateNeedRow(sid, x.mate) : mateReadyRow(sid, x.mate)}</div>`;
    return `<div class="cpn-row" tabindex="-1">${x.html || `<div class="mt-row"><div class="mt-n"><b>${esc(x.who)}</b> <i>${esc(x.what)}</i></div><div class="mt-btns">${x.buttons}</div></div>`}</div>`;
  };
  const body = list.length ? list.map(row).join("") : `<div class="cpn-none">${esc(crewNone(sid))}</div>`;
  return `<div class="cp-view cpn" data-cp-view="needs">${crewViewHead(sid, `<b class="cpv-name">Needs you</b><span class="cpv-n">${list.length || "nothing now"}</span>`)}<div class="cpv-scroll" data-scroll-key="cpn:${esc(sid)}">${body}</div></div>`;
}

/** The lead's box while the lead itself asks: held in place at its height, the draft kept, until you answer (or ↪
 *  puts its chip in it). `n`, `of`: where the lead's ask is in the bar. */
function crewHeldBox(sid, n, of, quick = true) {
  const why = of > 1 ? `The lead asks too (${n} of ${of}): answer it first.` : "The lead asks: answer it first.";
  return `<div class="foot held cp-held">${box_(`s:${sid}`, `${why} Your message waits here…`, "Answer first", `data-cp-first="${esc(sid)}"`, "", "", quick)}</div>`;
}
/** The box with ↪'s chip: its words go as that answer. */
function crewChipBox(sid, quick = true) {
  const it = findItem(crewChip.id);
  const place = it?.kind === "question" ? "Your own answer…" : "Tell it what to do instead…";
  return `<div class="foot cp-chipped"><div class="cp-chip"><span>${esc(crewChip.label)}</span><button class="cp-chip-x" data-cp-chip-x aria-label="Take the chip off">×</button></div>${box_(`s:${sid}`, place, "Send", `data-cp-chip-send="${esc(sid)}"`, "", "", quick)}</div>`;
}
/** The foot under the left on lead `sid`'s page (`quick`: with the chat's quick phrases, only while the left is the chat):
 *  the box with a chip, the held box while the lead asks, else null
 *  (the pane's own box). */
function crewFoot(sid, quick = true) {
  if (crewChip?.sid === sid && !isPending(findItem(crewChip.id))) crewChip = null;
  if (crewChip?.sid === sid) return crewChipBox(sid, quick);
  const list = crewAsks(sid), n = list.findIndex((x) => x.lead);
  return n >= 0 ? crewHeldBox(sid, n + 1, list.length, quick) : null;
}
/** ↪ on ask `id`: the chip in the box and the cursor there. */
function crewChipOn(sid, id) {
  const it = findItem(id);
  if (!it) return;
  const who = mateOfAsk(it) || "the lead";
  crewChip = { sid, id, label: it.kind === "question" ? `↪ your answer, to ${who}` : `↪ instead, to ${who}` };
  renderMain();
  focusComposer();
}
/** The chip's answer: the box's words, as the ask's own "tell it instead" (or your own answer to its question). */
function crewChipSend(sid) {
  const it = crewChip && findItem(crewChip.id), key = `s:${sid}`, text = draft(key).text.trim();
  if (!it || !text) return;
  draft(it.id).text = text;
  crewChip = null;
  draft(key).text = "";
  saveDrafts();
  submitText(it);
  renderMain();
  focusComposer();
}

/** The keys of the bar, from anywhere on the page: ⌘⇧A / ⌘⇧D answer the ask it shows (never a one-way one: its
 *  buttons carry no key), ⌘⇧↑ / ⌘⇧↓ step through the asks. With the chip on, Enter in the box sends its answer, and
 *  Backspace in the empty box takes the chip off. True when it took the key. */
function crewBarKey(e) {
  if (!crewPageSid || !document.querySelector(".crew-page") || sheet || lightbox) return false;
  const sid = crewPageSid, k = e.key?.toLowerCase(), box = e.target?.dataset?.text === `s:${sid}`;
  if (crewChip?.sid === sid && box && !e.isComposing) {
    if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey) { crewChipSend(sid); return true; }
    if (e.key === "Backspace" && !e.target.value) { crewChip = null; renderMain(); focusComposer(); return true; }
  }
  if (!e.metaKey || !e.shiftKey || e.altKey || e.ctrlKey) return false;
  if (k === "arrowup" || k === "arrowdown") { crewBarStep(sid, k === "arrowup" ? -1 : 1); return true; }
  if (k !== "a" && k !== "d") return false;
  const b = document.querySelector(`.cp-bar [data-bar-key="${k}"]`);
  if (b && !crewBarDead()) { b.click(); }
  return true;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (crewBarKey(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", (e) => {
    const t = e.target, sid = crewPageSid;
    if (!sid || !t.closest?.(".crew-page")) return;
    // The guard: a click on the bar's buttons in its first moment after a change goes nowhere.
    const bar = t.closest(".cp-bar");
    if (bar && t.closest("button") && !t.closest("[data-cp-step]") && crewBarDead()) { e.stopImmediatePropagation(); e.preventDefault(); return; }
    const el = t.closest("[data-cp-step], [data-cp-all], [data-cp-chip], [data-cp-chip-x], [data-cp-chip-send], [data-cp-first]");
    if (!el) {
      // An answer in the bar: the cursor goes to the box after it, never to the next ask's button.
      if (bar && t.closest("[data-act], [data-pick], [data-mt-act]")) setTimeout(focusComposer, 0);
      return;
    }
    e.stopImmediatePropagation();
    const d = el.dataset;
    if (d.cpStep) return crewBarStep(sid, +d.cpStep);
    if (d.cpAll) { crewLeftSet(sid, { kind: "needs" }); renderMain(); return document.querySelector(".cpn .cpn-row")?.focus(); }
    if (d.cpChip) return crewChipOn(sid, d.cpChip);
    if (el.matches("[data-cp-chip-x]")) { crewChip = null; renderMain(); return focusComposer(); }
    if (d.cpChipSend) return crewChipSend(sid);
    // Answer first (held, but clicked): the lead's ask in the bar.
    const lead = crewAsks(sid).find((x) => x.lead);
    if (lead) { crewBarShow(sid, lead.key); renderMain(); }
  }, true);
}
