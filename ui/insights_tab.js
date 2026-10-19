// Cue UI insights tab — the Insights view: what the last days of sessions cost and where it could have
// gone less, from each session's own log (the turns Cue keeps, logs/turns.rs).

/** The range shown, in days (7, 30 or 90): kept across launches. */
let itabDays = (() => { try { return +localStorage.getItem("cue.itabDays") || 30; } catch { return 30; } })();
/** What insights_summary said, per range: { at, data }; and the range being asked for now. */
const itabCache = new Map();
let itabAsking = 0;

/** The summary for the range shown: asked for when there's none or it's over a minute old (15 s while the
 *  history is still being read, so the numbers and the bar keep up). */
function itabData() {
  const c = itabCache.get(itabDays);
  if ((!c || Date.now() - c.at > (c.data?.reading ? 15000 : 60000)) && itabAsking !== itabDays) {
    const days = itabDays;
    itabAsking = days;
    invoke("insights_summary", { days }).then((data) => { itabCache.set(days, { at: Date.now(), data }); }).catch((e) => { itabCache.set(days, { at: Date.now(), data: c?.data || { days, totals: {}, error: String(e) } }); }).finally(() => { if (itabAsking === days) itabAsking = 0; renderMain(); });
  }
  return c?.data || null;
}

/** 1.2M, 340k, 900. */
function itabNum(n) {
  n = n || 0;
  if (n >= 1e9) return `${(n / 1e9).toFixed(n >= 1e10 ? 0 : 1)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(n >= 1e4 ? 0 : 1)}k`;
  return String(Math.round(n));
}

/** "2026-10-03" as "Oct 3". */
function itabDay(d) {
  const [y, m, day] = d.split("-").map(Number);
  return new Date(y, m - 1, day).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Every day of the range, oldest first, as "YYYY-MM-DD" in local time. */
function itabDaysList(days) {
  const out = [], d = new Date(now());
  d.setHours(12, 0, 0, 0);
  d.setDate(d.getDate() - (days - 1));
  for (let k = 0; k < days; k++) {
    out.push(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`);
    d.setDate(d.getDate() + 1);
  }
  return out;
}

/** A small line chart over the range, as insights.js's insChart draws one: `series` is [{ vals, dashed }],
 *  one value per day (null: no turns that day, the line skips it). The first series gets the fill and the dot. */
function itabChart(series, days, fmt) {
  const W = 600, H = 96, n = days.length;
  const top = Math.max(1, ...series.flatMap((s) => s.vals.filter((v) => v != null))) * 1.12;
  const X = (i) => (n > 1 ? (i / (n - 1)) * W : W / 2), Y = (v) => H - (v / top) * H;
  const pts = (s) => s.vals.map((v, i) => (v == null ? null : [X(i), Y(v)])).filter(Boolean);
  const path = (p) => (p.length ? `M${p.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(" L")}` : "");
  const main = pts(series[0]);
  const fill = main.length > 1 ? `${path(main)} L${main.at(-1)[0].toFixed(1)},${H} L${main[0][0].toFixed(1)},${H} Z` : "";
  const end = main.at(-1);
  const lines = series.map((s) => { const p = pts(s); return p.length > 1 ? `<path class="${s.dashed ? "line dashed" : "line"}" d="${path(p)}"></path>` : ""; }).join("");
  return `<div class="itab-chart">
    <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-hidden="true">
      <line class="base" x1="0" x2="${W}" y1="${H}" y2="${H}"></line>
      ${fill ? `<path class="fill" d="${fill}"></path>` : ""}${lines}
    </svg>
    <span class="itab-top mono">${esc(fmt(top / 1.12))}</span>
    ${end ? `<i class="itab-dot" style="left:${((end[0] / W) * 100).toFixed(1)}%;top:${((end[1] / H) * 100).toFixed(1)}%"></i>` : ""}
  </div>
  <div class="ins-axis"><span>${esc(itabDay(days[0]))}</span><span>${esc(itabDay(days.at(-1)))}</span></div>`;
}

/** One small card: a heading, its number now, the chart. */
function itabCard(head, note, chart) {
  return `<div class="itab-card"><div class="itab-head"><span class="itab-title">${esc(head)}</span><span class="itab-note">${note}</span></div>${chart}</div>`;
}

/** $470, $1,840, $4.20 (API list prices). */
function itabUsd(n) {
  n = n || 0;
  if (n > 0 && n < 10) return `$${n.toFixed(2)}`;
  return `$${Math.round(n).toLocaleString("en-US")}`;
}

/** How long a wait was: 50m, 14h, 2d 6h. */
function itabDur(ms) {
  const m = Math.round((ms || 0) / 60000);
  if (m < 60) return `${m}m`;
  if (m < 1440) return `${Math.floor(m / 60)}h${m % 60 && m < 600 ? ` ${m % 60}m` : ""}`;
  const d = Math.floor(m / 1440), h = Math.floor((m % 1440) / 60);
  return `${d}d${h ? ` ${h}h` : ""}`;
}

/** A prompt as you typed it: without a long block of instructions appended in brackets at its end (a
 *  dictation's or a tool's). Cue leaves them out when it reads a turn (steps.rs snippet); this is for rows
 *  read before it did, and for a snippet cut off inside one. */
function itabBare(text) {
  return String(text || "").replace(/\s*\([^()]{40,}\)?\s*$/, "").trim() || String(text || "");
}
/** "when" for a row: 3h ago, then Oct 3. */
function itabWhen(ms) {
  if (!ms) return "";
  return now() - ms < 7 * 86400000 ? `${ago(ms)} ago` : new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** The sessions the rows on screen name, by id: what opening one needs (its project, harness). */
const itabRows = new Map();

/** A session's row in a finding: project and name, a bar for its share of the finding, then its numbers
 *  (the first of them the one a narrow window leaves out). Clicking it opens the session. */
function itabSess(r, cells, share = null, at = null) {
  itabRows.set(r.session_id, r);
  const name = bareName(nameOf(r.session_id, r.project)) || r.project || "session";
  const proj = r.project && r.project !== name ? `<span class="itab-proj">${esc(r.project)}</span>` : "";
  const bar = share == null ? "" : `<span class="itab-bar"><i style="width:${Math.max(2, share * 100).toFixed(1)}%"></i></span>`;
  return `<button class="itab-row${bar ? " itab-row-bar" : ""}" data-itab-open="${esc(r.session_id)}"${at ? ` data-itab-at="${at}"` : ""}>
    <span class="itab-who">${proj}<span class="itab-name">${esc(name)}</span><span class="itab-ago">${esc(itabWhen(r.last_ms))}</span></span>
    ${bar}${cells.map((c, i) => `<span class="itab-cell${i ? "" : " itab-minor"}">${c}</span>`).join("")}
  </button>`;
}

/** One finding: its name, one big number and what it means, the evidence (the pictures, the sessions behind
 *  it), and the one thing to do about it. `more`: anything shown under the evidence, across the whole card. */
function itabFind(name, big, says, chart, evidence, todo, more = "", tag = "") {
  return `<section class="itab-find">
    <div class="itab-kicker">${esc(name)}${tag}</div>
    <div class="itab-claim"><b class="itab-big">${esc(big)}</b><p class="itab-says">${says}</p></div>
    ${chart || evidence ? `<div class="itab-evidence">${chart ? `<div class="itab-trend">${chart}</div>` : ""}${evidence ? `<div class="itab-list">${evidence}</div>` : ""}</div>` : ""}
    ${more}<p class="itab-do"><span class="itab-do-label">What to do</span>${todo}</p>
  </section>`;
}

/** A small heading over a picture, and a line saying what it shows. */
function itabFig(head, note, body) {
  return `<div class="itab-fig"><div class="itab-fighead"><span class="itab-figtitle">${esc(head)}</span>${note ? `<span class="itab-fignote">${note}</span>` : ""}</div>${body}</div>`;
}

/** The kinds a range's dollars split into, needed first, then the two avoidable ones. */
const itabKinds = [
  ["read", "Cache reads"], ["write", "Cache writes"], ["output", "Output"], ["fresh", "Fresh input"],
  ["rewrite", "Re-writes after waits", true], ["long", "Re-reads past hand-off", true],
];

/** Where the dollars went: one bar split by kind, the avoidable part hatched at its end, and a key with each
 *  kind's dollars and share. */
function itabSplit(split, total) {
  const sum = itabKinds.reduce((a, [k]) => a + (split[k] || 0), 0) || total || 1;
  const segs = itabKinds.filter(([k]) => split[k] > 0).map(([k]) => `<i class="itab-k-${k}" style="width:${((split[k] / sum) * 100).toFixed(2)}%"></i>`).join("");
  const key = itabKinds.map(([k, label, bad]) => `<span class="itab-key${bad ? " bad" : ""}"><i class="itab-k-${k}"></i><span class="itab-keyname">${esc(label)}</span><b>${esc(itabUsd(split[k]))}</b><span class="itab-keypct">${Math.round(((split[k] || 0) / sum) * 100)}%</span></span>`).join("");
  return `<div class="itab-split">${segs}</div><div class="itab-keys">${key}</div>`;
}

/** Each day's dollars as a column: the avoidable part (re-writes and re-reads) at its foot in amber, so days
 *  compare by it; the worst day named above its column. */
function itabDaily(days, byDay) {
  const rows = days.map((day) => { const r = byDay.get(day) || {}; const bad = (r.wait_usd || 0) + (r.long_usd || 0); return { day, usd: Math.max(r.usd || 0, bad), bad }; });
  const top = Math.max(1, ...rows.map((r) => r.usd)) * 1.1;
  const worst = rows.reduce((a, r) => (r.bad > a.bad ? r : a), rows[0]);
  const cols = rows.map((r) => `<span class="itab-col${r === worst && r.bad ? " worst" : ""}" style="height:${((r.usd / top) * 100).toFixed(1)}%"><i style="height:${r.usd ? ((r.bad / r.usd) * 100).toFixed(1) : 0}%"></i></span>`).join("");
  const wi = rows.indexOf(worst), left = ((wi + 0.5) / rows.length) * 100;
  const label = worst?.bad ? `<span class="itab-callout${left > 70 ? " end" : left < 30 ? " start" : ""}" style="left:${left.toFixed(1)}%;bottom:${((worst.usd / top) * 100).toFixed(1)}%">${esc(itabDay(worst.day))}: ${esc(itabUsd(worst.bad))} avoidable</span>` : "";
  return `<div class="itab-daily"><span class="itab-top mono">${esc(itabUsd(top / 1.1))}</span>${cols}${label}</div>
  <div class="ins-axis"><span>${esc(itabDay(days[0]))}</span><span>${esc(itabDay(days.at(-1)))}</span></div>`;
}

/** "claude-opus-5-5" as "Opus 5.5"; other models as they are. */
function itabModel(m) {
  const i = (m || "").indexOf("claude-");
  if (i < 0) return m || "unknown";
  const [fam, ...rest] = m.slice(i + 7).replace(/\[.*\]$/, "").split("-");
  const ver = rest.filter((x) => /^\d{1,2}$/.test(x)).join(".");
  return `${fam[0].toUpperCase()}${fam.slice(1)}${ver ? ` ${ver}` : ""}`;
}

/** Ranked bars: a name, a bar for its share of the biggest, and its numbers. `mark`: a tick across every
 *  track (an average to compare against), as a share of the biggest. */
function itabRank(rows, mark = null) {
  return `<div class="itab-rank">${rows.map((r) => `<span class="itab-rname">${esc(r.name)}</span>
    <span class="itab-rtrack">${mark != null ? `<b class="itab-rmark" style="left:${(mark * 100).toFixed(1)}%"></b>` : ""}<i class="${r.bad ? "bad" : ""}" style="width:${Math.max(1.5, r.share * 100).toFixed(1)}%"></i></span>
    <span class="itab-rval">${r.value}</span>`).join("")}</div>`;
}

/** When sessions were left waiting: a week of hours, each cell as amber as the dollars late answers to waits
 *  that began then cost. Monday first, midnight at the left. */
function itabWeek(grid, turns = []) {
  const top = Math.max(0.01, ...grid), sum = grid.reduce((a, x) => a + x, 0) || 1;
  const names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  const hr = (h) => `${(h + 11) % 12 + 1}${h % 24 < 12 ? "am" : "pm"}`;
  // Hovered, a box says itself in place of the figure's note (itabWeekHover): "Sun 6pm–7pm · 4 waits, $4.20 re-written (9%)".
  const say = (n, h, v, k) => `${n} ${hr(h)}–${hr(h + 1)} · ${v ? `${k ? `${k} wait${k === 1 ? "" : "s"}, ` : ""}${itabUsd(v)} re-written (${Math.round((v / sum) * 100)}%)` : "no waits began then"}`;
  const cells = names.map((n, d) => `<span class="itab-wday">${n}</span>${grid.slice(d * 24, d * 24 + 24).map((v, h) => `<i data-itab-cell="${esc(say(n, h, v, turns[d * 24 + h] || 0))}"${v ? ` style="background:color-mix(in srgb, var(--amber) ${Math.round(18 + Math.sqrt(v / top) * 82)}%, var(--panel-2))"` : ""}></i>`).join("")}`).join("");
  const hours = ["12am", "6am", "12pm", "6pm"].map((h) => `<span>${h}</span>`).join("");
  return `<div class="itab-week">${cells}</div><div class="itab-hours">${hours}</div>`;
}

/** When the wait began: the box under the pointer, said in place of the figure's note, which comes back on leaving. */
function itabWeekHover(e) {
  const cell = e.target.closest?.("[data-itab-cell]");
  const note = e.target.closest?.(".itab-fig")?.querySelector(".itab-fignote");
  if (!note || !e.target.closest(".itab-week")) return;
  if (note.dataset.was == null) note.dataset.was = note.textContent;
  note.textContent = cell ? cell.dataset.itabCell : note.dataset.was;
  note.classList.toggle("on", !!cell);
}
document.addEventListener("mouseover", itabWeekHover);
document.addEventListener("mouseout", (e) => {
  const week = e.target.closest?.(".itab-week");
  if (!week || week.contains(e.relatedTarget)) return;
  const note = week.closest(".itab-fig")?.querySelector(".itab-fignote");
  if (note?.dataset.was != null) { note.textContent = note.dataset.was; note.classList.remove("on"); }
});

/** Every late answer as a dot: across, how long it waited (a log scale, 5 minutes to days); up, how much it
 *  re-wrote; its size, what that cost. Three bands: what an hour's cache would have kept, what a hand-off
 *  would have, and the overnight ones. Clicking a dot opens its session at that turn. */
function itabDots(answers) {
  const M = 60000, lo = Math.log(5 * M), hi = Math.log(Math.max(3 * 86400000, ...answers.map((a) => a.waited_ms)) * 1.15);
  const X = (ms) => ((Math.log(Math.max(5 * M, ms)) - lo) / (hi - lo)) * 100;
  const yTop = Math.max(1, ...answers.map((a) => a.tokens)) * 1.08, uTop = Math.max(0.01, ...answers.map((a) => a.usd));
  const bands = [[5 * M, 60 * M, "5m – 1h"], [60 * M, 8 * 60 * M, "1h – 8h"], [8 * 60 * M, Infinity, "8h and more"]].map(([a, b, label]) => {
    const inIt = answers.filter((x) => x.waited_ms >= a && x.waited_ms < b), usd = inIt.reduce((s, x) => s + x.usd, 0);
    const l = X(a), r = b === Infinity ? 100 : X(b);
    return `<span class="itab-band" style="left:${l.toFixed(1)}%;width:${(r - l).toFixed(1)}%"><b>${esc(itabUsd(usd))}</b><span>${esc(label)} · ${inIt.length}</span></span>`;
  }).join("");
  const ticks = [[5 * M, "5m"], [60 * M, "1h"], [8 * 60 * M, "8h"], [86400000, "1d"], [3 * 86400000, "3d"]].map(([ms, t]) => `<span style="left:${X(ms).toFixed(1)}%">${t}</span>`).join("");
  const dots = [...answers].reverse().map((a) => {
    itabRows.set(a.session_id, itabRows.get(a.session_id) || a);
    const size = 5 + Math.sqrt(a.usd / uTop) * 15;
    return `<button class="itab-dot2" data-itab-open="${esc(a.session_id)}" data-itab-at="${a.at_ms || ""}" style="left:${X(a.waited_ms).toFixed(2)}%;bottom:${((a.tokens / yTop) * 100).toFixed(2)}%;width:${size.toFixed(1)}px;height:${size.toFixed(1)}px" aria-label="${esc(`${a.project}: waited ${itabDur(a.waited_ms)}, ${itabUsd(a.usd)}`)}"></button>`;
  }).join("");
  return `<div class="itab-bands">${bands}</div>
  <div class="itab-scatter"><span class="itab-top mono">${esc(itabNum(yTop / 1.08))} re-written</span>${dots}</div>
  <div class="itab-ticks">${ticks}</div>`;
}

/** A long session's climb: its context turn by turn, a dashed line where it started, a dotted one at 300k, a
 *  amber mark where it passed its hand-off point and the amber past it, what it re-read above where it started. */
function itabClimb(r) {
  itabRows.set(r.session_id, r);
  const name = bareName(nameOf(r.session_id, r.project)) || r.project || "session";
  const pts = r.climb || [], n = Math.max(2, r.climb_turns || pts.length), W = 300, H = 100;
  const top = Math.max(r.peak_context || 0, 300000, ...pts.map((p) => p[1])) * 1.08;
  const X = (i) => (i / (n - 1)) * W, Y = (v) => H - (v / top) * H, s0 = Y(r.start_context || 0);
  const line = pts.map(([i, v]) => `${X(i).toFixed(1)},${Y(v).toFixed(1)}`).join(" L");
  const past = r.handoff_turn != null ? pts.filter(([i]) => i >= r.handoff_turn) : [];
  const waste = past.length > 1 ? `<path class="waste" d="M${past.map(([i, v]) => `${X(i).toFixed(1)},${Y(v).toFixed(1)}`).join(" L")} L${X(past.at(-1)[0]).toFixed(1)},${s0.toFixed(1)} L${X(past[0][0]).toFixed(1)},${s0.toFixed(1)} Z"></path>` : "";
  const hx = r.handoff_turn != null ? (r.handoff_turn / (n - 1)) * 100 : null;
  const chart = pts.length > 1 ? `<div class="itab-climb">
    <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-hidden="true">
      <path class="area" d="M${line} L${X(pts.at(-1)[0]).toFixed(1)},${H} L${X(pts[0][0]).toFixed(1)},${H} Z"></path>${waste}
      <line class="start" x1="0" x2="${W}" y1="${s0.toFixed(1)}" y2="${s0.toFixed(1)}"></line>
      <line class="limit" x1="0" x2="${W}" y1="${Y(300000).toFixed(1)}" y2="${Y(300000).toFixed(1)}"></line>
      <path class="line" d="M${line}"></path>
      ${hx != null ? `<line class="mark" x1="${X(r.handoff_turn).toFixed(1)}" x2="${X(r.handoff_turn).toFixed(1)}" y1="0" y2="${H}"></line>` : ""}
    </svg>
    <span class="itab-limit mono" style="top:${((Y(300000) / H) * 100).toFixed(1)}%">300k</span>
    ${hx != null ? `<span class="itab-handoff${hx > 60 ? " end" : ""}" style="left:${hx.toFixed(1)}%">hand off here · turn ${r.handoff_turn + 1}</span>` : ""}
  </div>` : "";
  return `<button class="itab-tile" data-itab-open="${esc(r.session_id)}">
    <span class="itab-tilehead"><span class="itab-who">${r.project && r.project !== name ? `<span class="itab-proj">${esc(r.project)}</span>` : ""}<span class="itab-name">${esc(name)}</span></span><b>${esc(itabUsd(r.usd))}</b></span>
    ${chart}
    <span class="itab-tilefoot"><span>${esc(itabNum(r.start_context))} → ${esc(itabNum(r.peak_context))}</span><span>${r.turns} turn${r.turns === 1 ? "" : "s"} past it</span><span>${esc(itabWhen(r.last_ms))}</span></span>
  </button>`;
}

/** The Insights view: where the range's dollars went, then three findings (waiting past the cache, long
 *  sessions, corrections), each with the pictures that point at it, the sessions behind it, and what to do;
 *  then the plain trends. */
/** While Cue is still reading the sessions' history (a new Mac, years of logs): how far, as a bar. */
function itabReading(r) {
  if (!r?.total) return "";
  const pct = Math.min(99, Math.floor((r.done / r.total) * 100));
  return `<div class="itab-reading"><div class="itab-reading-t"><span>Reading your session history: what's here so far is part of it.</span><b>${pct}%</b></div><div class="itab-reading-bar"><i style="width:${pct}%"></i></div></div>`;
}
function insightsTabView() {
  const d = itabData();
  const ranges = `<div class="hfilter">${[7, 30, 90].map((n) => `<button class="hfchip ${itabDays === n ? "on" : ""}" data-itab-days="${n}">${n} days</button>`).join("")}</div>${itabReading(d?.reading)}`;
  if (!d) return `${ranges}<div class="itab"><div class="quiet-line itab-empty">Reading…</div></div>`;
  const t = d.totals || {}, w = d.waiting || {}, l = d.long || {}, c = d.corrections || {};
  const tile = (value, label) => `<div class="hs-tile"><b>${esc(value)}</b><span>${esc(label)}</span></div>`;
  const tiles = `<div class="hstats">
    ${tile(itabUsd(t.usd), `${itabNum(t.tokens)} tokens`)}
    ${tile(itabNum(t.turns), "turns")}
    ${tile(itabNum(t.sessions), "sessions")}
    ${tile(itabUsd((w.usd || 0) + (l.usd || 0)), "of it avoidable")}
  </div>`;
  if (d.error) return `${ranges}<div class="itab"><div class="quiet-line itab-empty">Couldn't read the turns: ${esc(d.error)}</div></div>`;
  if (!t.turns) return `${ranges}${tiles}<div class="itab"><div class="quiet-line itab-empty">No turns in the last ${d.days} days yet. Cue reads them from each session's log in the background, a little at a time.</div></div>`;
  const days = itabDaysList(d.days);
  const byDay = new Map((d.by_day || []).map((r) => [r.day, r]));
  const val = (k, gap) => days.map((day) => { const r = byDay.get(day); return r ? r[k] || 0 : gap ? null : 0; });
  const pct = (x) => `${Math.round((x || 0) * 100)}%`;
  const bad = (txt) => `<span class="itab-tag">${esc(txt)}</span>`;
  itabRows.clear();

  const priced = t.priced != null && t.priced < 0.95 ? ` Some models couldn't be priced (${pct(1 - t.priced)} of tokens), so the dollars run low.` : "";
  const note = `<div class="itab-fine">Dollars are API list prices. On a subscription they aren't a bill: they're what wears down your usage limits.${priced}</div>`;

  // Where it went: the split, each day, each model.
  const avoid = (w.usd || 0) + (l.usd || 0), sp = d.split || {};
  const ms = (d.models || []).filter((m) => m.usd > 0 || m.tokens > 0), mTop = Math.max(0.01, ...ms.map((m) => m.usd));
  const models = ms.length ? itabFig("By model", "", itabRank(ms.map((m) => ({ name: itabModel(m.model), share: m.usd / mTop, value: m.usd ? `<b>${esc(itabUsd(m.usd))}</b> ${pct(m.usd / (t.usd || 1))}` : `${esc(itabNum(m.tokens))} tokens, unpriced` })))) : "";
  const where = `<section class="itab-find itab-where">
    <div class="itab-kicker">Where it went</div>
    <div class="itab-claim"><b class="itab-big">${esc(itabUsd(t.usd))}</b><p class="itab-says">${avoid ? `<b class="itab-hot">${esc(itabUsd(avoid))}</b> of it (${pct(avoid / (t.usd || 1))}) was avoidable: caches re-written after a session waited on you, and long contexts re-read past their hand-off point.` : "Nothing in this range was avoidable by the measures below."}</p></div>
    ${Object.keys(sp).length ? itabSplit(sp, t.usd) : ""}
    <div class="itab-evidence itab-wide">${itabFig("Each day", "amber: the avoidable part", itabDaily(days, byDay))}${models}</div>
  </section>`;

  // Waiting past the cache: when, how long, which sessions.
  const ws = w.sessions || [], wTop = Math.max(0.01, ...ws.map((r) => r.usd));
  const grid = w.grid_usd || [], gSum = grid.reduce((a, x) => a + x, 0);
  const share = (f) => (gSum ? grid.reduce((a, x, i) => a + (f(Math.floor(i / 24), i % 24) ? x : 0), 0) / gSum : 0);
  const night = share((_, h) => h >= 18 || h < 6), weekend = share((wd) => wd >= 5);
  const peak = grid.indexOf(Math.max(...grid));
  const when = gSum ? `${pct(night)} began in the evening or overnight${weekend >= 0.15 ? `, ${pct(weekend)} on weekends` : ""}. Most: ${["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][Math.floor(peak / 24)]} around ${((peak % 24) + 11) % 12 + 1}${peak % 24 < 12 ? "am" : "pm"}.` : "";
  const ans = w.answers || [];
  const wPics = gSum || ans.length ? `<div class="itab-evidence itab-pics">${gSum ? itabFig("When the wait began", when, itabWeek(grid, w.grid_turns)) : ""}${ans.length ? itabFig("Each late answer", "across: how long it waited · up: what it re-wrote · size: what it cost", itabDots(ans)) : ""}</div>` : "";
  const waiting = w.turns
    ? itabFind("Waiting past the cache", itabUsd(w.usd),
      `Answering after a session's cache ran out re-wrote ${itabNum(w.tokens)} tokens: ${w.turns} late answer${w.turns === 1 ? "" : "s"}, ${pct(w.share_of_writes)} of all cache writes.`,
      "", "",
      "Answer before its cache runs out: the countdown on a waiting card says how long is left. For a session that's gone stale, start fresh from a hand-off instead of resuming it.",
      `${wPics}${ws.length ? itabFig("The sessions behind it", "", `<div class="itab-list">${ws.map((r) => itabSess(r, [`${r.times}×`, `waited up to ${itabDur(r.longest_wait_ms)}`, `<b>${itabUsd(r.usd)}</b>`], r.usd / wTop)).join("")}</div>`) : ""}`, bad("avoidable"))
    : itabFind("Waiting past the cache", "$0", "No answer came after its session's cache ran out.", "", "", "Keep answering before the countdown on a waiting card runs out.");

  // Long sessions: each one's climb, the hand-off point marked.
  const ls = l.sessions || [], ho = l.handoffs || { sent: 0, continued: 0 };
  const hoLine = ho.sent ? ` ${ho.sent} hand-off${ho.sent === 1 ? "" : "s"} sent, ${ho.continued} continued in a new session.` : " No hand-offs sent yet.";
  const long = itabFind("Long sessions", itabUsd(l.usd),
    `Sessions that ran on past their hand-off point re-read ${itabNum(l.tokens)} tokens of context above where they started.${hoLine}`,
    "", "",
    "Hand off when Cue offers it: past 300k and four times what it started with (150k and three times once its cache has run out), a fresh one with a short summary re-reads far less.",
    ls.length ? itabFig("How their context climbed", "amber: re-read above the start, past the hand-off point", `<div class="itab-tiles">${ls.slice(0, 6).map(itabClimb).join("")}</div>`) : "", l.usd ? bad("avoidable") : "");

  // Could have delegated: turns that looked around themselves, and long sessions across several areas.
  const dl = d.delegate || {}, dls = dl.sessions || [], dsp = dl.spread || [], dTop = Math.max(0.01, ...dls.map((r) => r.usd));
  const used = `Already delegated: ${itabNum(dl.agents || 0)} background agent${dl.agents === 1 ? "" : "s"} in ${itabNum(dl.agent_sessions || 0)} session${dl.agent_sessions === 1 ? "" : "s"}${dl.relay_spawns ? `, ${itabNum(dl.relay_spawns)} relay executor${dl.relay_spawns === 1 ? "" : "s"}` : ""}.`;
  const looked = dl.turns
    ? `${itabNum(dl.turns)} turn${dl.turns === 1 ? "" : "s"} did ${dl.looks_min} or more reads and searches themselves, each growing its session's context by ${itabNum(dl.growth_min)} or more; later replies re-read ${itabNum(dl.tokens)} tokens of it.`
    : "No turn did a lot of reading and searching itself.";
  const spreadLine = dl.spread_count ? ` ${dl.spread_count} session${dl.spread_count === 1 ? "" : "s"} ran past the hand-off point editing in ${dl.areas_min} or more areas.` : "";
  const delSess = dls.length ? itabFig("The turns that looked around", "reads and searches · what it grew · what was re-read", `<div class="itab-list">${dls.map((r) => itabSess(r, [`${r.looks} looks`, `+${itabNum(r.growth)}`, `<b>${itabUsd(r.usd)}</b>`], r.usd / dTop, r.at_ms)).join("")}</div>`) : "";
  const delSpread = dsp.length ? itabFig("Long, across several areas", "its re-reads past the hand-off point are counted under Long sessions", `<div class="itab-list">${dsp.map((r) => itabSess(r, [`peak ${itabNum(r.peak_context)}`, esc(r.areas.slice(0, 4).map(areaName).join(", ") + (r.areas.length > 4 ? ", …" : "")), `<b>${r.areas.length} areas</b>`])).join("")}</div>`) : "";
  const split = dl.relay
    ? "For work across several areas, start a relay lead (/relay:mode): each area gets an executor with a small context of its own."
    : "For work across several areas, a background agent per area, each in its own worktree, keeps every context small.";
  const deleg = itabFind("Could have delegated", itabUsd(dl.usd || 0),
    `${looked}${spreadLine} ${used}`,
    "", "",
    `Hand the looking to a background agent (“have an agent find where …”): it reads in a context of its own and sends back a summary. ${split}`,
    `${delSess || delSpread ? `<div class="itab-evidence itab-pics">${delSess}${delSpread}</div>` : ""}`, dl.usd ? bad("avoidable") : "");

  // Corrections: which projects, which prompts.
  const ex = c.examples || [];
  const pair = (e) => {
    itabRows.set(e.session_id, e);
    return `<button class="itab-pair" data-itab-open="${esc(e.session_id)}" data-itab-at="${e.at_ms || ""}">
      <span class="itab-meta">${esc(e.project || "")}<span>${esc(itabWhen(e.at_ms))}</span></span>
      <span class="itab-q itab-asked"><span class="itab-qlabel">You asked</span>${esc(itabBare(e.prompt))}</span>
      <span class="itab-q itab-then"><span class="itab-qmark" aria-hidden="true">“</span><span><span class="itab-qlabel">Then you said</span>${esc(itabBare(e.correction))}</span></span>
    </button>`;
  };
  const bp = (c.by_project || []).map((p) => ({ ...p, rate: p.turns ? p.count / p.turns : 0 })).sort((a, b) => b.rate - a.rate);
  const rTop = Math.max(0.001, ...bp.map((p) => p.rate), c.share || 0);
  const byProj = bp.length ? itabFig("By project", `line: your ${pct(c.share)} overall`, itabRank(bp.map((p) => ({ name: p.project, share: p.rate / rTop, bad: p.rate > (c.share || 0) * 1.5, value: `<b>${(p.rate * 100).toFixed(1)}%</b> ${p.count} of ${itabNum(p.turns)}` })), (c.share || 0) / rTop)) : "";
  const corr = itabFind("Corrections", itabNum(c.count),
    `${pct(c.share)} of turns: your next prompt corrected the one before.`,
    "", "",
    "Front-load the prompt: the file, what it should do, and how to check it worked.",
    `<div class="itab-evidence itab-pics">${byProj}${itabFig("Each day", "", itabChart([{ vals: val("corrected") }], days, (n) => String(Math.round(n))))}</div>${ex.length ? `<details class="itab-more"><summary>${ex.length} examples</summary><div class="itab-pairs">${ex.map(pair).join("")}</div></details>` : ""}`);

  const ctxs = (d.by_day || []).map((r) => r.median_context).filter((x) => x > 0);
  const midCtx = ctxs.length ? [...ctxs].sort((a, b) => a - b)[Math.floor(ctxs.length / 2)] : 0;
  const ins = (d.insights || []).filter((i) => i.shown).map((i) => `${esc(i.rule)}: acted on ${i.acted} of ${i.shown}${i.tokens ? ` · saved ${itabNum(i.tokens)} tokens` : ""}`).join(" · ");
  const trends = `<div class="itab-sub">Trends</div><div class="itab-grid">
    ${itabCard("Tokens per day", `${itabNum(t.tokens / d.days)} a day`, itabChart([{ vals: val("tokens") }], days, itabNum))}
    ${itabCard("Median context per turn", midCtx ? `around ${itabNum(midCtx)}` : "", itabChart([{ vals: val("median_context", true) }], days, itabNum))}
    ${itabCard("Turns per day", `${(t.turns / d.days).toFixed(t.turns / d.days < 10 ? 1 : 0)} a day`, itabChart([{ vals: val("turns") }], days, (n) => String(Math.round(n))))}
  </div>${ins ? `<div class="itab-fine">Cue's insights: ${ins}</div>` : ""}`;

  return `${ranges}${tiles}<div class="itab">${note}${where}${trends}${waiting}${long}${deleg}${corr}</div>`;
}

/** A session's row clicked: a live one opens in Active, as search opens it; an ended one opens its whole
 *  conversation (at that turn, for a correction). */
document.addEventListener("click", (e) => {
  const b = e.target.closest?.("[data-itab-open]");
  if (!b) return;
  const sid = b.dataset.itabOpen, r = itabRows.get(sid) || { session_id: sid };
  if (sessionOf(sid)) return openSession(sid);
  if ((state.live || []).some((q) => q.session_id === sid)) return svAct("quiet", sid);
  // Parked or just closed: its pane, with what resuming costs beside Resume and Start fresh.
  if (parkedOf(sid) || closedOf(sid)) return setActive(null, sid);
  openConvo({ session_id: sid, name: bareName(nameOf(sid, r.project)) || r.project, project: r.project, harness: r.harness }, +b.dataset.itabAt || null, "Insights");
});

/** A range picked: kept, and the view drawn for it. */
document.addEventListener("click", (e) => {
  const b = e.target.closest?.("[data-itab-days]");
  if (!b) return;
  itabDays = +b.dataset.itabDays;
  try { localStorage.setItem("cue.itabDays", String(itabDays)); } catch {}
  renderMain();
});
