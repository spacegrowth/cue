// Cue UI board: usage limits (a paused session's note, Resend) and the usage meters in the header.
// ---------- usage limits ----------
/** A limit that has lifted (its reset time passed, or another turn went through). */
const lifted = (l) => !!l?.resets_ms && l.resets_ms <= now();
const limitedSessions = () => state.sessions.filter((s) => s.state === "limited" && s.limit);
/** "4:10am" today (or within the next day), "Sat 8am" further out. */
function clockAt(ms) {
  const d = new Date(ms), h = d.getHours() % 12 || 12, m = d.getMinutes(), ap = d.getHours() < 12 ? "am" : "pm";
  const t = m ? `${h}:${String(m).padStart(2, "0")}${ap}` : `${h}${ap}`;
  return ms - now() < 20 * 3600000 ? t : `${d.toLocaleDateString(undefined, { weekday: "short" })} ${t}`;
}
const untilText = (ms) => { const m = Math.round(ms / 60000); return m < 1 ? "under a minute" : m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${m % 60}m`; };
const scopeName = (l) => l.scope === "all" ? "Claude Code" : l.scope;
function limitNote(s) {
  const l = s.limit;
  if (!l) return "out of usage";
  if (lifted(l)) return "usage is back · resend";
  if (l.scope !== "all") return `${l.scope} used up${l.resets_ms ? ` until ${clockAt(l.resets_ms)}` : ""}`;
  return `out of usage${l.resets_ms ? ` · resets ${clockAt(l.resets_ms)}` : ""}`;
}
/** Resend what each paused session didn't get to run. */
async function resend(sids) {
  let ok = 0;
  for (const sid of sids) {
    const s = sessionOf(sid);
    if (!s?.prompt) continue;
    try { await invoke("send_to_session", { sessionId: sid, text: s.prompt, images: [], now: false }); ok++; }
    catch (e) { toast(`Couldn't resend to ${s.project}: ${e}`); }
  }
  if (ok) toast(ok === 1 ? "Resent" : `Resent to ${ok} sessions`);
}
let usageOpen = false;
/** Next to Board / History: the plan's usage, or loudly, that it's used up. */
function usageMeter(label, w) {
  return w ? `<span class="ul">${esc(label)}</span><span class="um ${w.pct >= 100 ? "full" : w.pct >= 85 ? "high" : ""}"><i style="width:${Math.min(100, w.pct)}%"></i></span><span class="up">${Math.round(w.pct)}%</span>` : "";
}
/** The plan has no such limit (Ultra has no 5-hour one): the last full report from Claude Code left it out. */
const noLimit = (u, k) => !!u?.no_limit?.includes(k);
/** Claude Code's usage has something to show: a window's number, or that a window doesn't apply. */
const claudeHasUsage = (u) => !!(u && (u.five_hour || u.seven_day || u.no_limit?.length));
function codexUsageWindows() {
  if (state.settings?.usage?.codex === false) return [];
  return (state.codex_usage?.windows || []).filter((w) => !w.resets_ms || w.resets_ms > now());
}
function usageWindowName(minutes, short = false) {
  if (minutes === 10080) return short ? "wk" : "Weekly";
  if (minutes % 1440 === 0) return short ? `${minutes / 1440}d` : `${minutes / 1440} ${minutes === 1440 ? "day" : "days"}`;
  if (minutes % 60 === 0) return short ? `${minutes / 60}h` : `${minutes / 60} ${minutes === 60 ? "hour" : "hours"}`;
  return short ? `${minutes}m` : `${minutes} ${minutes === 1 ? "minute" : "minutes"}`;
}
function usageChip() {
  const windows = codexUsageWindows();
  const claude = claudeUsageChip();
  if (!windows.length) return claude;
  const claudeRates = state.settings?.usage?.claude !== false ? state.usage : null;
  if (claudeHasUsage(claudeRates) && !limitedSessions().length) {
    const claudeFive = claudeRates.five_hour;
    const codexFive = windows.find((w) => w.minutes === 300);
    const provider = (name, w, none = false) => `<span class="uc-one"><span class="usage-provider">${name}</span>${none ? '<span class="up">no limit</span>' : w ? `<span class="um ${w.pct >= 100 ? "full" : w.pct >= 85 ? "high" : ""}"><i style="width:${Math.min(100, w.pct)}%"></i></span><span class="up">${Math.round(w.pct)}%</span>` : '<span class="up">—</span>'}</span>`;
    return `<span class="usage-wrap"><button class="usage-chip usage-combined" data-usage="meter" aria-label="Usage" aria-expanded="${usageOpen === "claude"}">${provider("Claude", claudeFive, noLimit(claudeRates, "five_hour"))}${provider("Codex", codexFive)}<span class="usage-window">5h</span></button>${usagePop()}</span>`;
  }
  const label = claude ? '<span class="usage-provider">Codex</span>' : "";
  const headerWindows = claude ? windows.filter((w) => w.minutes === 300) : windows;
  const codex = `<span class="usage-wrap"><button class="usage-chip" data-usage="codex" aria-label="Codex usage" aria-expanded="${usageOpen === "codex"}">${label}${headerWindows.map((w) => usageMeter(usageWindowName(w.minutes, true), w)).join("")}</button>${codexUsagePop(windows)}</span>`;
  return claude ? `<span class="usage-meters">${claude}${codex}</span>` : codex;
}
function codexUsagePop(windows) {
  if (usageOpen !== "codex") return "";
  return `<div class="usage-pop">${codexUsageDetails(windows)}</div>`;
}
function codexUsageDetails(windows) {
  if (!windows.length) return "";
  const rates = state.codex_usage;
  return `<div class="up-head">Codex usage<span class="dim">${ago(rates.at_ms)} ago</span></div>${windows.map((w) => `<div class="up-group"><div class="up-title">${usageWindowName(w.minutes)}${w.resets_ms ? `<span> · resets ${clockAt(w.resets_ms)}</span>` : ""}</div><div class="up-row ${w.pct >= 100 ? "full" : w.pct >= 85 ? "high" : ""}"><span class="up-name">Used</span><span class="up-bar"><i style="width:${Math.min(100, w.pct)}%"></i></span><span class="up-pct">${Math.round(w.pct)}%</span></div></div>`).join("")}<div class="up-foot">From local Codex sessions · updates as Codex reports usage.</div>`;
}
function claudeUsageChip() {
  const limited = limitedSessions();
  const out = limited.filter((s) => s.limit.scope === "all" && !lifted(s.limit));
  const back = limited.filter((s) => lifted(s.limit) && now() - s.limit.resets_ms < 5 * 60000);
  const model = limited.filter((s) => s.limit.scope !== "all" && !lifted(s.limit));
  const paused = (n) => `${n} paused`;
  if (out.length) {
    const r = Math.max(...out.map((s) => s.limit.resets_ms || 0));
    return `<span class="usage-wrap"><button class="usage-out" data-usage="out"><span class="uo-dot"></span>Claude Code out of usage<span class="uo-sub">${r ? ` · resets ${clockAt(r)}` : ""} · ${paused(out.length)}</span></button>${usagePop()}</span>`;
  }
  if (back.length) return `<span class="usage-wrap"><span class="usage-back"><span class="ub-dot"></span>${esc(scopeName(back[0].limit))} is back<button class="ub-btn" data-usage="resend">Resend ${back.length}</button></span></span>`;
  if (model.length) {
    const l = model[0].limit;
    return `<span class="usage-wrap"><button class="usage-chip soft-warn" data-usage="model"><span class="uf-dot"></span>${esc(l.scope)} used up<span class="uo-sub">${l.resets_ms ? ` · ${clockAt(l.resets_ms)}` : ""} · ${paused(model.length)}</span></button>${usagePop()}</span>`;
  }
  const u = state.usage;
  const uf = state.usage_file || {};
  // Settings → Usage can hide Claude Code's limits (e.g. behind a proxy, where only your file matters).
  const claude = state.settings?.usage?.claude !== false && claudeHasUsage(u);
  if (!claude && !(uf.meters || []).length && !uf.error) return "";
  const meter = usageMeter;
  // Your own meters (Settings → Usage file) sit beside Claude Code's, drawn the same way: the first
  // row in the bar, and all of them in a list of the same look that hovering it shows.
  const level = (pct) => pct === null ? "" : pct >= 100 ? "full" : pct >= 85 ? "high" : "";
  const fileMeter = (m, short) => {
    const pct = m.limit ? (m.spent / m.limit) * 100 : null;
    const label = short && m.label.length > 12 ? m.label.slice(0, 11) + "…" : m.label;
    return `${m.label ? `<span class="ul">${esc(label)}</span>` : ""}${pct === null ? "" : `<span class="um ${level(pct)}"><i style="width:${Math.min(100, pct)}%"></i></span>`}<span class="up">${esc(spendText(m))}</span>`;
  };
  const meters = uf.meters || [];
  const mine = meters.length ? `<span class="uf-first">${fileMeter(meters[0], true)}</span>` : "";
  const more = meters.length > 1 && !usageOpen ? `<span class="uf-more">${meters.map((m) => `<span class="uf-row">${fileMeter(m)}${m.resets_ms ? `<span class="uf-note">resets ${clockAt(m.resets_ms)}</span>` : ""}</span>`).join("")}</span>` : "";
  const broken = uf.error ? `<span class="ul uf-bad" title="${esc(uf.error)}">usage file ⚠</span>` : "";
  return `<span class="usage-wrap"><button class="usage-chip" data-usage="meter" aria-label="${claude ? "Claude Code usage" : "Your usage"}" aria-expanded="${usageOpen === "claude"}">${claude && codexUsageWindows().length ? '<span class="usage-provider">Claude</span>' : ""}${claude ? claudeMeters(u, !codexUsageWindows().length) : ""}${mine}${broken}</button>${more}${usagePop()}</span>`;
}
/** Claude Code's windows in the header: each one's bar, or "no limit" for one the plan doesn't have.
 *  `weekly`: the weekly one too (left out beside Codex, whose 5-hour one sits next to it). */
function claudeMeters(u, weekly) {
  const one = (label, k) => noLimit(u, k) ? `<span class="ul">${label}</span><span class="up">no limit</span>` : usageMeter(label, u[k]);
  return one("5h", "five_hour") + (weekly ? one("wk", "seven_day") : "");
}
/** "$12.40/$50", "48/200 credits", "$3.10": an amount from your usage file, in its unit. */
function spendText(m, long = false) {
  const n = (x) => Number(x).toLocaleString(undefined, { maximumFractionDigits: 2, minimumFractionDigits: Number.isInteger(x) ? 0 : 2 });
  const sym = m.unit.length <= 2 && !/[a-z]/i.test(m.unit);
  const amt = (x) => (sym ? `${m.unit}${n(x)}` : n(x));
  const both = m.limit ? `${amt(m.spent)}${long ? " of " : "/"}${amt(m.limit)}` : amt(m.spent);
  return sym ? both : `${both} ${m.unit}`;
}
function usagePop() {
  const u = state.usage;
  const uf = state.usage_file || {};
  if (usageOpen !== "claude") return "";
  // Your usage file: its meters with their details, or what's wrong with it.
  const fileSec = !(uf.meters || []).length && !uf.error ? "" : `<div class="up-group"><div class="up-title">${uf.error ? "Usage file" : "Your usage"}<span> · ${esc(uf.path || "")}${uf.as_of_ms ? `, as of ${clockAt(uf.as_of_ms)}` : ""}</span></div>
    ${uf.error ? `<div class="up-err">${esc(uf.error)}</div>` : uf.meters.map((m) => {
      const pct = m.limit ? (m.spent / m.limit) * 100 : null;
      const lvl = pct === null ? "" : pct >= 100 ? "full" : pct >= 85 ? "high" : "";
      return `<div class="up-row money ${lvl}"><span class="up-name">${esc(m.label || "Usage")}</span>${pct === null ? `<span class="up-bar none"></span>` : `<span class="up-bar"><i style="width:${Math.min(100, pct)}%"></i></span>`}<span class="up-pct">${esc(spendText(m, true))}</span>${m.resets_ms ? `<span class="up-note">resets ${clockAt(m.resets_ms)}</span>` : ""}</div>`;
    }).join("")}</div>`;
  const claude = state.settings?.usage?.claude !== false && claudeHasUsage(u);
  const codexSec = codexUsageDetails(codexUsageWindows());
  if (!claude) return `<div class="usage-pop">${codexSec || '<div class="up-head">Usage</div>'}${fileSec}</div>`;
  const lvl = (w) => (w.pct >= 100 ? "full" : w.pct >= 85 ? "high" : "");
  const row = (name, w, note = "") => w ? `<div class="up-row ${lvl(w)}"><span class="up-name">${esc(name)}</span><span class="up-bar"><i style="width:${Math.min(100, w.pct)}%"></i></span><span class="up-pct">${Math.round(w.pct)}%</span>${note ? `<span class="up-note">${esc(note)}</span>` : ""}</div>` : "";
  // Rows that reset together sit under one heading that says when, instead of repeating it per row.
  const group = (title, w, rows) => rows ? `<div class="up-group"><div class="up-title">${title}${w?.resets_ms ? `<span> · resets ${clockAt(w.resets_ms)}</span>` : ""}</div>${rows}</div>` : "";
  // Per-model rows come from Claude Code's /usage panel, which only refreshes while it's open.
  const models = (u.models || []).map((m) => {
    const notes = [m.resets_ms && Math.abs(m.resets_ms - (u.seven_day?.resets_ms || 0)) > 3600e3 ? `resets ${clockAt(m.resets_ms)}` : "", now() - m.fetched_ms > 15 * 60000 ? `${ago(m.fetched_ms)} old` : ""];
    return row(m.name, m, notes.filter(Boolean).join(" · "));
  }).join("");
  // A limit the plan doesn't have says so, instead of a bar.
  const none = (k, what) => noLimit(u, k) ? `<div class="up-row up-none"><span class="up-name">${what}</span></div>` : "";
  const plan = u.plan ? ` · ${esc(u.plan)}` : "";
  return `<div class="usage-pop"><div class="up-head">Claude Code usage${plan}<span class="dim">${ago(u.at_ms)} ago</span></div>
    ${group("Session", u.five_hour, row("5 hours", u.five_hour) || none("five_hour", "No 5-hour limit on your plan"))}${group("Weekly", u.seven_day, (row("All models", u.seven_day) || none("seven_day", "No weekly limit on your plan")) + models)}
    ${codexSec ? `<div class="up-provider">${codexSec}</div>` : ""}${fileSec}</div>`;
}
