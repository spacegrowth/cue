// Cue UI chat — /mcp (or ⋯ → MCP servers…) in a hosted Claude Code session: its MCP servers as Claude Code has them
// right now (mcp_live), and what Claude Code's own /mcp screen does to them, done from here (mcp_act): sign in,
// reconnect, turn on or off (at once: nothing restarts), sign out. src-tauri/src/ipc/mcp_live.rs asks Claude Code.
// Signing in opens the server's page in your browser; a session on another machine asks for the address the browser
// ended on (mcp_signed_in). A server's state is asked again every few seconds while a sign-in is under way.
const mcplCache = new Map();   // sid → { data, err, loading, at }
const mcplBusy = new Set();    // "<sid>|<server>": something asked of it, not yet answered
const mcplSigning = new Map(); // "<sid>|<server>": { url, paste, typed, until } while a sign-in is under way
const mcplOpen = new Set();    // "<sid>|<server>": its tools unfolded
let mcplPasteN = 0;
const MCPL_SIGN_WAIT = 180000, MCPL_POLL = 2500;
const mcplKey = (sid, name) => `${sid}|${name}`;

const MCPL_FRESH = 30000;
/** Ask Claude Code again (`ifOld`: only when what's here is older than MCPL_FRESH). */
function mcplLoad(sid, ifOld = false) {
  const c = mcplCache.get(sid) || mcplCache.set(sid, { data: null, err: "", loading: false, at: 0 }).get(sid);
  if (c.loading || (ifOld && now() - c.at < MCPL_FRESH)) return;
  c.loading = true;
  invoke("mcp_live", { sessionId: sid }).then((v) => { c.data = v; c.err = ""; }, (x) => { c.err = String(x || "couldn't ask it"); })
    .finally(() => { c.loading = false; c.at = now(); mcplSettleSignIns(sid); renderMain(); });
}

/** A sign-in is over once its server is connected (or it's been waited on long enough); while one is under way, ask
 *  again shortly. */
function mcplSettleSignIns(sid) {
  const servers = mcplCache.get(sid)?.data?.servers || [];
  let waiting = false;
  for (const [k, s] of mcplSigning) {
    if (!k.startsWith(`${sid}|`)) continue;
    const m = servers.find((x) => mcplKey(sid, x.name) === k);
    if (m?.status === "connected" || now() > s.until) mcplSigning.delete(k);
    else waiting = true;
  }
  if (waiting) setTimeout(() => { if (moreFor === sid && mcpPicking) mcplLoad(sid); }, MCPL_POLL);
}

/** What a server's state reads as, and its colour. */
function mcplState(m, signing) {
  if (signing) return [signing.paste ? "finish signing in, then paste the address below" : "finish signing in in your browser…", "warn"];
  return ({ connected: ["connected", "ok"], "needs-auth": ["needs sign-in", "warn"], pending: ["connecting…", ""], failed: ["didn't connect", "bad"], disabled: ["off", ""] })[m.status] || [m.status || "unknown", ""];
}

/** One server: on/off, its name and state, its tools, and what can be done to it now. */
function mcplRow(sid, m) {
  const k = mcplKey(sid, m.name), busy = mcplBusy.has(k), signing = mcplSigning.get(k);
  const [word, tone] = mcplState(m, signing);
  const on = m.status !== "disabled";
  const btn = (act, label) => `<button class="mcpl-act" data-mcpl="${act}" data-sid="${esc(sid)}" data-name="${esc(m.name)}" ${busy ? "disabled" : ""}>${label}</button>`;
  const acts = busy ? `<span class="mcpl-wait">…</span>`
    : !on ? ""
    : m.status === "needs-auth" ? btn("sign-in", signing ? "Sign in again" : "Sign in")
    : m.status === "failed" ? btn("reconnect", "Reconnect")
    : m.status === "connected" ? `${btn("reconnect", "Reconnect")}${m.can_sign_out ? btn("sign-out", "Sign out") : ""}` : "";
  const tools = m.tools || [], open = mcplOpen.has(k);
  const fold = tools.length ? `<button class="mcp-tfold" data-mcpl="tools" data-sid="${esc(sid)}" data-name="${esc(m.name)}" aria-expanded="${open}">· ${tools.length} tool${tools.length === 1 ? "" : "s"} ${open ? "▴" : "▾"}</button>` : "";
  const list = open && tools.length ? `<div class="mcp-tools mono">${tools.map((t) => `<span>${esc(t)}</span>`).join("")}</div>` : "";
  const paste = signing?.paste ? `<div class="mcpl-paste"><input id="mcpl-paste-${signing.n}" data-mcpl-paste="${esc(k)}" placeholder="The address your browser ended on" value="${esc(signing.typed || "")}"/><button class="btn small primary" data-mcpl="pasted" data-sid="${esc(sid)}" data-name="${esc(m.name)}" ${busy || !signing.typed ? "disabled" : ""}>Done</button></div>` : "";
  const why = m.status === "failed" && m.error ? `<div class="mcpl-why">${esc(m.error)}</div>` : "";
  // Its name and what can be done to it, then under the name its state, tools and why it failed.
  return `<div class="mcpl-it"><label class="mcpl-on" title="${on ? "On: turn it off" : "Off: turn it on"}"><input type="checkbox" data-mcpl-on="${esc(sid)}" value="${esc(m.name)}" ${on ? "checked" : ""} ${busy ? "disabled" : ""}/></label><span class="mcp-nm ${on ? "" : "off"}">${esc(m.title || m.name)}</span><span class="grow"></span>${acts}</div><div class="mcpl-sub"><span class="mcp-live ${tone}">${esc(word)}</span>${fold}</div>${why}${list}${paste}`;
}

/** ⋯ → MCP servers…: how many are connected, and how many wait for a sign-in or didn't connect. */
function mcplItem(sid) {
  mcplLoad(sid, true);
  const servers = mcplCache.get(sid)?.data?.servers;
  const count = (st) => (servers || []).filter((m) => m.status === st).length;
  const extra = servers ? [[count("needs-auth"), "need sign-in"], [count("failed"), "didn't connect"]].filter(([n]) => n).map(([n, w]) => ` · ${n} ${w}`).join("") : "";
  const label = servers ? `MCP servers · ${count("connected")} connected${extra}` : "MCP servers…";
  return `<button data-mcp-ask="${esc(sid)}">${esc(label)}<span>Sign in, reconnect, turn on or off: nothing restarts</span></button>`;
}

/** The panel: every server Claude Code has, then a way to restart the session (for servers added to its settings
 *  since it started, which only a start reads). */
function mcpLivePanel(sid) {
  const c = mcplCache.get(sid);
  if (!c) mcplLoad(sid);
  const servers = c?.data?.servers || [];
  const body = c?.err ? `<div class="mcp-none">${esc(c.err)}</div>`
    : !c?.data ? `<div class="mcp-none">Asking Claude Code…</div>`
    : !servers.length ? `<div class="mcp-none">No MCP servers are set up in Claude Code</div>`
    : `<div class="mcp-list" data-mcp-key="live:${esc(sid)}">${servers.map((m) => mcplRow(sid, m)).join("")}</div>`;
  // Picked for it alone (--strict-mcp-config): the folder's others and Claude Code's own aren't in it at all.
  const pinned = c?.data?.pinned ? `<div class="mcp-note">It runs only the servers picked for it, so others (the claude.ai ones too) aren't listed. <button class="mcp-back" data-mcpl="unpin" data-sid="${esc(sid)}" ${sessionOf(sid)?.state === "working" ? "disabled" : ""}>Use all of the folder's servers</button> (it restarts)</div>` : "";
  return `<div class="more-menu mcp-pick"><div class="pm-title">MCP servers<span class="grow"></span><button class="mcpl-act" data-mcpl="refresh" data-sid="${esc(sid)}" ${c?.loading ? "disabled" : ""}>Refresh</button></div>${body}${pinned}
    <div class="mcp-note">Changes apply now; nothing restarts. A server added to Claude Code's settings since it started needs a restart.</div>
    <div class="mcp-acts"><button class="mcp-back" data-mcpl="restart" data-sid="${esc(sid)}" ${sessionOf(sid)?.state === "working" ? "disabled" : ""}>Restart session</button><span class="grow"></span><button class="btn small" data-mcp="cancel">Close</button></div></div>`;
}

/** `action` on server `name` of `sid`, then its state asked again. */
async function mcplDo(sid, name, action) {
  const k = mcplKey(sid, name);
  mcplBusy.add(k);
  renderMain();
  try {
    const r = await invoke("mcp_act", { sessionId: sid, server: name, action });
    if (action === "sign-in") {
      if (!r?.url) throw new Error("Claude Code gave no sign-in link");
      mcplSigning.set(k, { url: r.url, paste: !!r.paste_back, typed: "", n: ++mcplPasteN, until: now() + MCPL_SIGN_WAIT });
      await invoke("open_link", { url: r.url });
    }
  } catch (x) { toast(`${name}: ${x?.message || x}`); }
  mcplBusy.delete(k);
  mcplLoad(sid);
}

document.addEventListener("input", (e) => {
  const p = e.target.closest?.("[data-mcpl-paste]");
  if (!p) return;
  const s = mcplSigning.get(p.dataset.mcplPaste);
  if (s) { s.typed = p.value; renderMain(); }
});
document.addEventListener("change", (e) => {
  const cb = e.target.closest?.("[data-mcpl-on]");
  if (cb) mcplDo(cb.dataset.mcplOn, cb.value, cb.checked ? "on" : "off");
});
document.addEventListener("click", async (e) => {
  const el = e.target.closest("[data-mcpl]");
  if (!el) return;
  const { mcpl: act, sid, name } = el.dataset;
  if (act === "refresh") return mcplLoad(sid);
  if (act === "tools") { const k = mcplKey(sid, name); mcplOpen.has(k) ? mcplOpen.delete(k) : mcplOpen.add(k); return renderMain(); }
  if (act === "unpin") { mcpClose(sid); renderMain(); mcplCache.delete(sid); return invoke("mcp_apply_clear", { sessionId: sid }).then(toast, (x) => toast(`Couldn't: ${x}`)); }
  if (act === "restart") { mcpClose(sid); renderMain(); return invoke("restart_session", { sessionId: sid }).then(toast, (x) => toast(`Couldn't restart it: ${x}`)); }
  if (act === "pasted") {
    const k = mcplKey(sid, name), s = mcplSigning.get(k);
    if (!s?.typed) return;
    mcplBusy.add(k);
    renderMain();
    try { await invoke("mcp_signed_in", { sessionId: sid, server: name, url: s.typed }); mcplSigning.delete(k); }
    catch (x) { toast(`${name}: ${x}`); }
    mcplBusy.delete(k);
    return mcplLoad(sid);
  }
  mcplDo(sid, name, act);
});
