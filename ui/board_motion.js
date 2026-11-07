// Cue UI board — its motion and fit: cards that glide where they moved, names fitted to their line, the lists'
// "more below" pills, the header chips, and answered cards folding away.
/** Cards that move between redraws (back into Waiting, into Need to decide, or up to
 *  close a gap) glide from where they were instead of jumping, so you can follow where they went.
 *  Keyed by what they show: an ask or turn by its id, a session card by its session. */
const FLIP_SEL = ".board .col:not(.main) .nrow[data-big], .board .col:not(.main) .nrow[data-session]:not(.talk-work), .board .col:not(.main) .working[data-session]";
const flipKey = (el) => (el.dataset.big ? `i:${el.dataset.big}` : `s:${el.dataset.session}`);
const COL_SEL = ".board > .col";
const colKey = (el) => "col:" + (el.classList.contains("main") ? "main" : el.classList.contains("rail-col") ? "rail" : el.classList.contains("col-waiting") ? "waiting" : "sessions");
function flipRects() {
  const m = new Map();
  for (const el of document.querySelectorAll(FLIP_SEL)) m.set(flipKey(el), el.getBoundingClientRect());
  for (const el of document.querySelectorAll(COL_SEL)) m.set(colKey(el), el.getBoundingClientRect());
  return m;
}
function flipPlay(from) {
  if (!from.size || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  for (const el of document.querySelectorAll(FLIP_SEL)) {
    const was = from.get(flipKey(el));
    if (!was) continue;
    const now = el.getBoundingClientRect(), dx = was.left - now.left, dy = was.top - now.top;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1) continue;
    el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: 340, easing: "cubic-bezier(.2,.8,.2,1)" });
  }
  // Columns glide when the board reflows (a section folds to the rail, or opens back out); a column
  // that just appeared (the rail, or a section re-opened) slides in from the right.
  for (const el of document.querySelectorAll(COL_SEL)) {
    const was = from.get(colKey(el));
    if (!was) { el.animate([{ opacity: 0, transform: "translateX(24px)" }, { opacity: 1, transform: "none" }], { duration: 240, easing: "ease-out" }); continue; }
    const now = el.getBoundingClientRect(), dx = was.left - now.left, dy = was.top - now.top;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1) continue;
    el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: 340, easing: "cubic-bezier(.2,.8,.2,1)" });
  }
}
/** The top bar never pushes Settings off the edge (a usage file adds meters; chips come and go): when it
 *  doesn't fit, it gives up room step by step: the chips' extras ("· oldest 4m"), the meters' bars, the
 *  meters' labels, then the chips' words (the number and dot stay; the tooltip still says what they are). */
const TOP_STEPS = ["tfit1", "tfit2", "tfit3", "tfit4"];
function fitTop() {
  const h = document.querySelector(".app > .top");
  if (!h) return;
  h.classList.remove(...TOP_STEPS);
  // Fits: Settings (the last button) ends inside the bar. Measured by it, so an open drop-down doesn't count.
  const fits = () => !h.lastElementChild || h.lastElementChild.getBoundingClientRect().right <= h.getBoundingClientRect().right + 1;
  for (const step of TOP_STEPS) {
    if (fits()) return;
    h.classList.add(step);
  }
}
/** A project's swatches never get cut off: near the foot of the projects list (or the window), they open
 *  upward instead. */
function placeColors() {
  const pop = document.querySelector(".pc-pop");
  if (!pop) return;
  const box = pop.closest(".pj-pop")?.getBoundingClientRect() ?? { top: 0, bottom: innerHeight };
  const r = pop.getBoundingClientRect(), btn = pop.previousElementSibling.getBoundingClientRect();
  pop.classList.toggle("up", r.bottom > box.bottom && btn.top - r.height - 6 >= box.top);
}
/** The chat header's buttons never get cut off and its chips never run into each other: when they don't fit, it
 *  gives up room step by step (short labels, then the agent and model, then the "your turn" pill, then the
 *  "N agents" chip, then the role tag, then the other chips, and the name shortens). The CSS widths can't know how
 *  many buttons and chips a session has. */
const FIT_STEPS = ["fit1", "fit2", "fit3", "fit4", "fit5", "fit6"];
function fitHead() {
  const h = document.querySelector(".ap-head");
  if (!h) return;
  h.classList.remove(...FIT_STEPS);
  // Fits: the last button ends inside the header's right padding.
  const fits = () => {
    const last = [...h.children].filter((k) => k.offsetParent).at(-1);
    return !last || last.getBoundingClientRect().right <= h.getBoundingClientRect().right - parseFloat(getComputedStyle(h).paddingRight) + 1;
  };
  for (const step of FIT_STEPS) {
    if (fits()) return;
    h.classList.add(step);
  }
}

/** A list cut off at the bottom says so, quietly: "3 more" on its last card's bottom edge, counting
 *  the cards not in view below, and a click takes the list to its end. And no card shows half: the list
 *  ends on the last card that fits whole. The lists that scroll in the third column: Sessions, Starred,
 *  Recently answered, Parked (not Waiting and Need to decide: their column scrolls as one, every card in
 *  it, nothing cut short). `trim` re-fits the lists (a redraw, a resize); scrolling only recounts. The
 *  pill shows itself when it appears or its count changes, then fades after a few seconds so a still list
 *  stays clean; scrolling the list (`woke`) brings it back. */
const MORE_LISTS = ".sec-sessions .sec-body, .sec-starred .card.recent, .sec-recent .card.recent, .sec-parked .sec-body";
const moreCards = (sc) => [...sc.children].filter((c) => !c.matches(".col-sub, button"));
// Where a card ends inside its list, by layout (the list is its offsetParent), not by what's on screen:
// cards that moved slide to their new place after a redraw, and mid-slide they're elsewhere.
const cardEnd = (c) => c.offsetTop + c.offsetHeight;
function markMore(trim = true, woke = moreWoke) {
  const lists = [...document.querySelectorAll(MORE_LISTS)].map((sc) => ({ sc, host: sc.closest(".sec") }));
  if (trim) {
    // Lists share their column: trimming one gives the next more room, and a section can spill past
    // the column's bottom. So: from scratch, then a few passes until nothing changes, each list ending
    // on its last whole card above its own edge and the column's (less room for "N more").
    for (const { sc } of lists) sc.style.maxHeight = "";
    for (let pass = 0; pass < 4; pass++) {
      let changed = false;
      for (const { sc, host } of lists) {
        const r = sc.getBoundingClientRect(), colBottom = sc.closest(".col").getBoundingClientRect().bottom;
        if (sc.scrollHeight - sc.clientHeight <= 4 && r.bottom <= colBottom + 1) continue;
        const room = Math.min(r.bottom, colBottom) - r.top;   // the list's own height it can use
        const whole = moreCards(sc).filter((c) => cardEnd(c) - sc.scrollTop <= room + 1).at(-1);
        const h = whole ? cardEnd(whole) - sc.scrollTop + 2 : 0;
        if (h > 40 && (!sc.style.maxHeight || h < parseFloat(sc.style.maxHeight) - 0.5)) { sc.style.maxHeight = `${h}px`; changed = true; }
      }
      if (!changed) break;
    }
  }
  for (const { sc, host } of lists) {
    let pill = host.querySelector(":scope > .more-below");
    const bottom = sc.getBoundingClientRect().bottom;
    const n = sc.scrollHeight - sc.scrollTop - sc.clientHeight > 4 ? moreCards(sc).filter((c) => cardEnd(c) > sc.scrollTop + sc.clientHeight + 1).length : 0;
    // A redraw makes a new pill each time: when it was shown is kept by list, not by element.
    const key = [...host.classList].find((c) => c.startsWith("sec-")) || host.className;
    if (!n) { pill?.remove(); clearTimeout(moreShown.get(key)?.timer); moreShown.delete(key); continue; }
    if (!pill) {
      pill = document.createElement("button");
      pill.className = "more-below";
      host.append(pill);
    }
    pill.list = sc;
    const label = `${n} more ↓`;
    let shown = moreShown.get(key);
    // Scrolled: by you, to somewhere else. A redraw puts the list back where it was, which fires a scroll
    // too; that one isn't a move, or the pill would never fade while sessions work (a redraw a second).
    const moved = woke.has(sc) && Math.abs(sc.scrollTop - (shown?.top ?? sc.scrollTop)) > 1;
    if (!shown || shown.label !== label || moved) {
      clearTimeout(shown?.timer);
      shown = { label, until: Date.now() + MORE_SHOWN_MS, timer: setTimeout(() => markMore(false), MORE_SHOWN_MS + 20) };
      moreShown.set(key, shown);
    }
    shown.top = sc.scrollTop;
    pill.classList.toggle("quiet", Date.now() >= shown.until);
    pill.textContent = label;
    pill.style.top = `${bottom - host.getBoundingClientRect().top - 10}px`;   // on the last card's bottom edge
  }
  woke.clear();
}
// Each list's pill: what it said, until when it shows (then it fades, until the list scrolls or the count
// changes), and where its list was scrolled to when last looked at (`top`).
const MORE_SHOWN_MS = 4000;
const moreShown = new Map();
// Scrolling a list changes what's below it: once a frame at most, for the lists that scrolled (`moreWoke`).
let moreFrame = 0;
const moreWoke = new Set();

// ---------- header chips: what's waiting, what's asking, what's going cold; click to open the oldest ----------
/** Each chip's list, oldest first (going cold: soonest first): what a click steps through. */
function chipList(kind) {
  const { yours, decide } = groups();
  if (kind === "waiting") return yours.map((i) => ({ id: i.id, sid: i.session_id, project: i.project, since: i.created_ms }));
  if (kind === "asking") return decide.map((i) => ({ id: i.id, sid: i.session_id, project: i.project, since: i.created_ms }));
  // Waiting on you with its cache counting down on its card (insights.js cacheWarn): the soonest to run out first.
  return [...yours, ...decide].map((i) => ({ id: i.id, sid: i.session_id, project: i.project, w: cacheWarn(i.session_id) }))
    .filter((x) => x.w && x.w.left > 0)
    .map((x) => ({ id: x.id, sid: x.sid, project: x.project, since: x.w.c.until_ms })).sort((a, b) => a.since - b.since);
}
function headerChips() {
  const chip = (kind, label, extra, cls, dot) => {
    const list = chipList(kind);
    if (!list.length) return "";
    // The chip you're stepping through: ‹ 2 of 3 waiting › (← → do the same).
    const at = chipNav === kind ? chipAt(list) : -1;
    if (at >= 0 && list.length > 1) return `<span class="hchip nav ${cls}"><button class="hstep" data-chip-step="${kind}:-1" title="Previous (←)" aria-label="Previous">‹</button><button class="hnav" data-chip="${kind}"><span class="hdot ${dot}"></span>${at + 1} of ${list.length}<span class="hlbl"> ${label}</span></button><button class="hstep" data-chip-step="${kind}:1" title="Next (→)" aria-label="Next">›</button></span>`;
    return `<button class="hchip ${cls}" data-chip="${kind}"><span class="hdot ${dot}"></span>${list.length}<span class="hlbl"> ${label}</span>${extra ? `<span class="hsub"> · ${extra}</span>` : ""}</button>`;
  };
  const waiting = chipList("waiting"), cold = chipList("cold");
  const oldest = waiting[0] ? now() - waiting[0].since : 0;
  // Stronger the longer the oldest has waited: soft under 5 minutes, then half, then solid at 15.
  const heat = oldest >= 15 * 60000 ? "hot" : oldest >= 5 * 60000 ? "warm" : "";
  const html = chip("waiting", "waiting", waiting[0] ? `oldest ${ago(waiting[0].since)}` : "", heat, "z")
    + chip("cold", "going cold", cold[0] ? `<span data-cold-in="${cold[0].since}">${coldIn(cold[0].since)}</span>` : "", "cold", "c")
    + chip("asking", "asking you", "", "", "ask");
  return html || `<span class="hall" data-tauri-drag-region>all clear</span>`;
}
/** Where the Active pane sits in a chip's list (-1 when it's showing something else). */
const chipAt = (list) => list.findIndex((x) => (x.id ? x.id === active?.id : x.sid === active?.sid && !isPending(findItem(active?.id))));
/** A chip click opens the oldest of its kind; clicking again (or ›, →) steps to the next, ‹ or ← back; both wrap. */
function openFromChip(kind, dir = 1) {
  const list = chipList(kind);
  if (!list.length) return;
  const at = chipAt(list);
  const next = list[at < 0 ? (dir > 0 ? 0 : list.length - 1) : (at + dir + list.length) % list.length];
  chipNav = kind;
  slideDir = dir;
  setActive(next.id, next.sid);
}

// ---------- boot ----------
const LINGER_MS = 1500;
/** A "✓ answered" card's time is up: it fades and folds away, and the cards below close the gap. */
function leaveGhosts() {
  const gone = [...ghosts].filter(([, g]) => Date.now() - g.at >= LINGER_MS).map(([id]) => id);
  if (!gone.length) return;
  const finish = () => { folding = false; heldDraw = false; gone.forEach((id) => ghosts.delete(id)); render(); };
  const els = gone.map((id) => document.querySelector(`.nrow.ghost[data-detail="${CSS.escape(id)}"]`)).filter(Boolean);
  if (!els.length || matchMedia("(prefers-reduced-motion: reduce)").matches) return finish();
  // Fades out first, then closes up, eating the column's 10px gap too, so nothing below jumps at the end.
  // Redraws wait meanwhile (sessions at work redraw often): one mid-fold would pop the row back.
  folding = true;
  for (const el of els) {
    const cs = getComputedStyle(el), h = `${el.offsetHeight}px`;
    el.style.overflow = "hidden";
    el.animate([
      { height: h, opacity: 0.6, paddingTop: cs.paddingTop, paddingBottom: cs.paddingBottom, marginBottom: "0px", offset: 0 },
      { height: h, opacity: 0, paddingTop: cs.paddingTop, paddingBottom: cs.paddingBottom, marginBottom: "0px", offset: 0.4 },
      { height: "0px", opacity: 0, paddingTop: "0px", paddingBottom: "0px", marginBottom: "-10px", offset: 1 },
    ], { duration: 420, easing: "ease-in-out", fill: "forwards" });
  }
  setTimeout(finish, 430);
}
