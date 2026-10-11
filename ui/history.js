// Cue UI history — the History view.
/** An opened History row: the conversation as it stood, then (for a decision) what it asked, read-only. */
function histDetail(i) {
  // A reply goes in the conversation, right under what it answered, so opening the row lands on it.
  const reply = i.kind === "waiting" && /^replied: “/.test(outcomeText(i)) ? replyOf(i) : null;
  const yours = reply ? `<div class="cv-you hreply"><div class="cv-you-text">${esc(reply).replace(/\n/g, "<br>")}</div>${thumbs(i.images)}<div class="cv-meta">You replied · ${ago(i.resolved_ms)} ago</div></div>` : "";
  const chat = (i.thread || []).length || i.kind === "waiting" ? `<div class="hdetail">${chatHtml(i, null, i.harness)}${yours}</div>` : "";
  let ask = "";
  if (i.kind === "question") ask = questions(i).map((q) => `<div class="qtext">${esc(q.question)}</div><ul class="hopts">${(q.options || []).map((o) => `<li>${esc(o.label)}${o.description ? `<span class="dim">: ${esc(o.description)}</span>` : ""}</li>`).join("")}</ul>`).join("");
  else if (i.kind !== "waiting" && !(inTerminal(i) && !i.tool_name)) ask = requestBody(i);
  return chat + (ask ? `<div class="hask"><div class="act-head">${esc(agentName(i.harness))} ${esc(verb(i))}${machTag(i.machine)}</div>${ask}</div>` : "");
}
/** What you replied, in full: from the live session's own log when it's still there (the stored outcome
 *  keeps only the first line), else the outcome's quote. */
function replyOf(i) {
  const e = (sessionOf(i.session_id)?.thread || []).find((x) => x.role === "you" && Math.abs(x.at_ms - i.resolved_ms) < 5000);
  return e?.text || (outcomeText(i).match(/^replied: “([\s\S]*)”/) || [])[1] || "";
}
/** Today at a glance, above History: how many you answered, how long they
 *  typically waited and the longest wait, what you did, and answers per hour over the last 12 hours. */
function histStats() {
  const midnight = new Date(now()); midnight.setHours(0, 0, 0, 0);
  // Every answer of the last day (state.answers), not the History list: that one is capped (Settings → History).
  const all = state.answers || state.history;
  const today = all.filter((i) => (i.resolved_ms || 0) >= midnight.getTime());
  const waits = today.map((i) => ({ i, w: (i.resolved_ms || 0) - i.created_ms })).filter((x) => x.w >= 0).sort((a, b) => a.w - b.w);
  const fmt = (ms) => { const m = Math.round(ms / 60000); return m < 1 ? "<1m" : m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${m % 60}m`; };
  const longest = waits.at(-1);
  const n = (re) => today.filter((i) => re.test(i.outcome || "")).length;
  const hour0 = new Date(now()); hour0.setMinutes(0, 0, 0);
  const hours = Array.from({ length: 12 }, (_, k) => {
    const from = hour0.getTime() - (11 - k) * 3600000, d = new Date(from);
    return { label: `${d.getHours() % 12 || 12}${d.getHours() < 12 ? "am" : "pm"}`, n: all.filter((i) => (i.resolved_ms || 0) >= from && (i.resolved_ms || 0) < from + 3600000).length };
  });
  const peak = Math.max(1, ...hours.map((h) => h.n));
  const tile = (value, label, title = "") => `<div class="hs-tile"><b>${esc(value)}</b><span>${esc(label)}</span></div>`;
  return `<div class="hstats">
    ${tile(String(today.length), "answered today")}
    ${tile(waits.length ? fmt(waits[Math.floor(waits.length / 2)].w) : "–", "median wait", "From when it asked to when you answered")}
    ${tile(longest ? fmt(longest.w) : "–", longest ? `longest · ${nameOf(longest.i.session_id, longest.i.project)}` : "longest wait")}
    ${tile(`${n(/^replied/)} · ${n(/^(allowed|answered)/)} · ${n(/^denied/)}`, "replied · allowed · denied")}
    <div class="hs-chart" role="img" aria-label="Answers by hour: ${esc(hours.map((h) => `${h.label} ${h.n}`).join(", "))}">
      <div class="hs-bars">${hours.map((h) => `<i class="${h.n ? "" : "zero"}" style="height:${h.n ? Math.max(4, (h.n / peak) * 100) : 6}%" title="${h.label} · ${h.n} answered"></i>`).join("")}</div>
      <div class="hs-axis"><span>${hours[0].label}</span><span>answers by hour</span><span>now</span></div>
    </div></div>`;
}

let histFilter = "all", histSession = "";  // History's filters: what kind, which session
const histOpenGroups = new Set();          // "DAY|session" groups you opened
const HIST_KINDS = [["all", "All"], ["waiting", "Replies"], ["permission", "Permissions"], ["question", "Questions"]];
/** History: what you answered, by day, and within a day by session (a session's answers fold into one
 *  row you can open). Filter by kind or session; times are clock times. */
function historyView() {
  const keep = state.settings?.history?.keep ?? 300;
  const day = (ms) => {
    const d = new Date(ms), today = new Date(now());
    const diff = Math.round((new Date(today.toDateString()) - new Date(d.toDateString())) / 86400000);
    return diff === 0 ? "TODAY" : diff === 1 ? "YESTERDAY" : d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" }).toUpperCase();
  };
  const at = (i) => i.resolved_ms || i.created_ms;
  const clock = (ms) => new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const inSession = (i) => (!histSession || i.session_id === histSession) && inProj(i.project);
  const count = (k) => state.history.filter((i) => (k === "all" || i.kind === k) && inSession(i)).length;
  const sessions = [...new Map(state.history.map((i) => [i.session_id, nameOf(i.session_id, i.project)])).entries()];
  const filters = `<div class="hfilter">${HIST_KINDS.map(([k, l]) => `<button class="hfchip ${histFilter === k ? "on" : ""}" data-hfilter="${k}">${l}<span>${count(k)}</span></button>`).join("")}
    <span class="grow"></span><select class="hsess" data-hsession aria-label="Session"><option value="">All sessions</option>${sessions.map(([sid, n]) => `<option value="${esc(sid)}" ${histSession === sid ? "selected" : ""}>${esc(n)}</option>`).join("")}</select></div>`;
  const row = (i, nested) => {
    const open = histOpen === i.id;
    return `<div class="hrow ${open ? "open" : ""} ${nested ? "nested" : ""}" data-hrow="${esc(i.id)}">
      <span class="hwho">${nested ? "" : `${projDot(i.project)}${badge(i.harness)}${nameSpan(i.session_id, i.project)}`}</span>
      <span class="hwhat ${isBash(i) ? "mono" : ""}">${esc(plain(summary(i)))}</span>
      <span class="hout ${outcomeClass(i)}">${esc(outcomeText(i))}</span>
      <span class="age">${clock(at(i))}</span></div>${open ? `<div class="hexpand">${histDetail(i)}${/^replied: “/.test(outcomeText(i)) ? "" : resultLine(i)}</div>` : ""}`;
  };
  // Day, then session in order of its latest answer.
  const groups = [];
  for (const i of state.history.filter((x) => (histFilter === "all" || x.kind === histFilter) && inSession(x))) {
    const d = day(at(i));
    let g = groups.find((x) => x.day === d && x.sid === i.session_id);
    if (!g) groups.push((g = { day: d, sid: i.session_id, items: [] }));
    g.items.push(i);
  }
  let html = `<div class="hhead"><span>session</span><span>what it wanted</span><span>you</span><span>when</span></div>`, lastDay = "";
  for (const g of groups) {
    if (g.day !== lastDay) { html += `<div class="hgroup">${g.day}</div>`; lastDay = g.day; }
    if (g.items.length === 1) { html += row(g.items[0], false); continue; }
    const key = `${g.day}|${g.sid}`, open = histSession || histOpenGroups.has(key), top = g.items[0];
    html += `<div class="hrow hsrow ${open ? "open" : ""}" data-hgroup="${esc(key)}">
      <span class="hwho"><span class="hfold">${open ? "▾" : "▸"}</span>${projDot(top.project)}${badge(top.harness)}${nameSpan(g.sid, top.project)}<span class="hcount">${g.items.length}</span></span>
      <span class="hwhat">${esc(plain(summary(top)))}</span>
      <span class="hout ${outcomeClass(top)}">${esc(outcomeText(top))}</span>
      <span class="age">${clock(at(g.items[g.items.length - 1]))}–${clock(at(top))}</span></div>`;
    if (open) html += g.items.map((i) => row(i, true)).join("");
  }
  if (!state.history.length) html += `<div class="quiet-line" style="padding:18px">Nothing answered yet.</div>`;
  else if (!groups.length) html += `<div class="quiet-line" style="padding:18px">Nothing here with these filters.</div>`;
  else html += `<div class="hfoot">${state.history.length} of the last ${keep.toLocaleString()} kept · change in Settings</div>`;
  return `${histStats()}${filters}<div class="hist">${html}</div>`;
}
