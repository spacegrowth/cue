// Cue UI chat — ⋯ → MCP servers.
// ---------- MCP servers: ⋯ → MCP servers… ----------
// Every server Claude Code knows for the session's folder (mcp_for), each with a checkbox: checked =
// it runs with it (`selected`). Restart session with these pins the session to the checked ones (mcp_apply: Cue
// restarts it with them, conversation kept, and every later resume keeps them); "Back to the folder's
// servers" drops that (mcp_apply_clear). The + New session form shows the same list for the folder typed.
const mcpCache = new Map();   // sid or "f:<folder>" → { at, data, loading, err }
const MCP_FRESH = 30000;
function mcpLoad(sid, force = false) {
  const c = mcpCache.get(sid) || { at: 0, data: null, loading: false };
  mcpCache.set(sid, c);
  if (c.loading || (!force && now() - c.at < MCP_FRESH)) return;
  c.loading = true;
  // A key "f:<folder>" is a folder with no session yet (the + New session form): its config alone.
  (sid.startsWith("f:") ? invoke("mcp_for_folder", { cwd: sid.slice(2) }) : invoke("mcp_for", { sessionId: sid })).then((v) => { c.data = v || { servers: [] }; c.err = ""; c.at = now(); c.loading = false; renderMain(); }, (x) => { c.loading = false; c.at = now(); c.err = String(x || "couldn't read them"); renderMain(); });
}
let mcpPicking = false;      // the ⋯ menu shows the MCP picker instead of its items
const mcpPick = new Map();   // key → Set of checked names, once you've changed a box (else what it has now)
const mcpToolsOpen = new Set();   // "<key>|<server>": its tools unfolded in the list
const mcpPickable = (m) => m.scope !== "session" && !m.blocked;   // any server with a definition can be passed; Claude Code's own can't, nor one a settings file blocks
function mcpChecked(key) {
  if (mcpPick.has(key)) return mcpPick.get(key);
  const servers = mcpCache.get(key)?.data?.servers || [];
  return new Set(servers.filter((m) => mcpPickable(m) && (m.selected ?? m.enabled)).map((m) => m.name));
}
/** How many it runs with, for the ⋯ item (null until read). */
function mcpOnCount(sid) {
  const d = mcpCache.get(sid)?.data;
  return d ? (d.servers || []).filter((m) => mcpPickable(m) && (m.selected ?? m.enabled)).length : null;
}
/** Where a server comes from, in a word or two. */
function mcpScopeWord(m) {
  if (m.scope === "user") return "always";
  if (m.scope === "local") return "this folder";
  if (m.scope === "project") return "project";
  if (m.scope === "session") return "Claude Code's";
  const from = String(m.from || "");
  if (/\.claude\.json$/.test(from)) return "this folder";   // turned off here, kept by Cue
  if (/\/mcp\/[^/]+\.json$/.test(from) && !/\.mcp\.json$/.test(from)) return "picked before";
  const dir = from.replace(/ \(local\)$/, "").replace(/\/\.mcp\.json$/, "").replace(/\/+$/, "");
  return `from ${dir.split("/").pop() || dir}`;
}
/** The checklist: pickable servers first, then Claude Code's own (shown, not checkable). */
function mcpList(key, compact = false) {
  mcpLoad(key);
  const c = mcpCache.get(key);
  if (!c?.data) return `<div class="mcp-none">${c?.err ? esc(`Couldn't read them: ${c.err}`) : "Reading…"}</div>`;
  const servers = c.data.servers || [];
  const on = mcpChecked(key);
  const item = (m) => {
    const can = mcpPickable(m);
    const n = m.used ? `<span class="mcp-n ${m.failed > 0 ? "bad" : ""}">· ${m.used} call${m.used === 1 ? "" : "s"}</span>` : "";
    // The tools the running session loaded from it, by name (logs/mcp.rs with_live), with the calls it
    // made to each: a click unfolds them.
    const tools = m.loaded || [], open = mcpToolsOpen.has(`${key}|${m.name}`);
    const calls = (t) => (m.tools || []).find((u) => u.name === t)?.calls || 0;
    const fold = tools.length ? `<button class="mcp-tfold" data-mcp="tools" data-sid="${esc(key)}" data-name="${esc(m.name)}" aria-expanded="${open}">· ${tools.length} tool${tools.length === 1 ? "" : "s"} ${open ? "▴" : "▾"}</button>` : "";
    const list = open && tools.length ? `<div class="mcp-tools mono">${tools.map((t) => `<span>${esc(t)}${calls(t) ? `<span class="mcp-tn"> · ${calls(t)}</span>` : ""}</span>`).join("")}</div>` : "";
    return `<label class="mcp-it ${can ? "" : "fixed"}"><input type="checkbox" data-mcp-pick="${esc(key)}" value="${esc(m.name)}" ${(can ? on.has(m.name) : m.selected ?? m.enabled) ? "checked" : ""} ${can ? "" : "disabled"}/><span class="mcp-nm">${esc(m.name)}</span><span class="mcp-sc">${esc(mcpScopeWord(m))}</span>${mcpLiveWord(m, key)}${n}${fold}</label>${list}`;
  };
  const mine = servers.filter((m) => m.scope !== "session"), theirs = servers.filter((m) => m.scope === "session");
  if (!mine.length && !theirs.length) return `<div class="mcp-none">No MCP servers are set up in Claude Code</div>`;
  return `<div class="mcp-list ${compact ? "compact" : ""}" data-mcp-key="${esc(key)}">${mine.map(item).join("")}${theirs.length ? `<div class="mcp-sub">Claude Code adds these itself</div>${theirs.map(item).join("")}` : ""}</div>`;
}
/** What the running session really has from a server (logs/mcp.rs with_live), so a tick never passes
 *  for working: connected, waiting for sign-in, connecting, failed, or (one it should have) not loaded. */
function mcpLiveWord(m, key) {
  // Blocked by a settings file's deniedMcpServers: Claude Code never loads it, ticked or not. The file, from its folder on.
  if (m.blocked) return `<span class="mcp-live bad">· blocked in ${esc(m.blocked.split("/").slice(-3).join("/"))}</span>`;
  // Signing in is Claude Code's own screen: a hosted session has none, so it's run in a terminal.
  const signIn = isHostedClaude(key) ? "needs sign-in: run claude /mcp in a terminal, then restart" : "needs sign-in: /mcp in its terminal";
  const w = { on: ["connected", "ok"], "sign-in": [signIn, "warn"], connecting: ["connecting", ""], failed: ["didn't connect", "bad"] }[m.live]
    || (m.live === "off" && (m.selected ?? m.enabled) ? ["not loaded", "bad"] : null);
  return w ? `<span class="mcp-live ${w[1]}">· ${w[0]}</span>` : "";
}
/** ⋯ → MCP servers…: the checklist, Restart session with these, Cancel, and (pinned) back to the folder's. A hosted
 *  Claude Code session's is Claude Code's own list instead (chat/mcp_live.js). */
function mcpPicker(sid) {
  if (isHostedClaude(sid)) return mcpLivePanel(sid);
  const busy = sessionOf(sid)?.state === "working";
  const d = mcpCache.get(sid)?.data;
  const back = d?.pinned ? `<button class="mcp-back" data-mcp="clear" data-sid="${esc(sid)}" ${busy ? "disabled" : ""}>Back to the folder's servers</button>` : "";
  // Claude Code only records what connected once it's had a turn: just restarted, it can't be told yet.
  const unknown = d?.running && !d.live_known ? `<div class="mcp-note">Whether each one connected shows after its next message</div>` : "";
  return `<div class="more-menu mcp-pick"><div class="pm-title">MCP servers</div>${mcpList(sid)}${unknown}
    ${busy ? `<div class="mcp-note">It's working; apply when it's idle</div>` : `<div class="mcp-note">Only this session restarts, with the ones checked; its conversation stays</div>`}
    <div class="mcp-acts">${back}<span class="grow"></span><button class="btn small" data-mcp="cancel">Cancel</button><button class="btn small primary" data-mcp="apply" data-sid="${esc(sid)}" ${busy || !d ? "disabled" : ""}>Restart session with these</button></div></div>`;
}
/** Open the ⋯ menu's MCP servers list for `sid` (⋯ → MCP servers…, or /mcp typed in a hosted session). */
function openMcp(sid) {
  moreFor = sid; handoffArm = null; mcpPicking = true; mcpPick.delete(sid);
  // A hosted Claude Code session: what it has right now, from Claude Code itself (chat/mcp_live.js).
  isHostedClaude(sid) ? mcplLoad(sid) : mcpLoad(sid, true);
  renderMain();
}
const isHostedClaude = (sid) => { const s = sessionOf(sid); return !!s?.hosted && s.harness === "claude"; };
function mcpClose(sid) { moreFor = handoffArm = null; mcpPicking = false; if (sid) mcpPick.delete(sid); }
document.addEventListener("change", (e) => {
  const cb = e.target.closest?.("[data-mcp-pick]");
  if (!cb) return;
  const key = cb.dataset.mcpPick;
  const set = new Set(mcpChecked(key));
  cb.checked ? set.add(cb.value) : set.delete(cb.value);
  mcpPick.set(key, set);
});
document.addEventListener("click", async (e) => {
  const ask = e.target.closest("[data-mcp-ask]");
  if (ask) return openMcp(ask.dataset.mcpAsk);
  const el = e.target.closest("[data-mcp]");
  if (!el) return;
  const k = el.dataset.mcp, sid = el.dataset.sid;
  if (k === "cancel") { mcpClose(moreFor); return renderMain(); }
  if (k === "tools") { e.preventDefault(); const t = `${sid}|${el.dataset.name}`; mcpToolsOpen.has(t) ? mcpToolsOpen.delete(t) : mcpToolsOpen.add(t); return renderMain(); }
  if (k === "apply" || k === "clear") {
    const names = [...mcpChecked(sid)];
    mcpClose(sid);
    renderMain();
    try { toast(await (k === "apply" ? invoke("mcp_apply", { sessionId: sid, names }) : invoke("mcp_apply_clear", { sessionId: sid }))); }
    catch (x) { toast(`Couldn't: ${x}`); }
    return mcpLoad(sid, true);
  }
});
