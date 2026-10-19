// Cue UI — extensions. An extension whose manifest has a "tab" adds that tab to the open session,
// in the row under its activity bar (Chat | Changes | …). The tab's content is the extension's own page,
// in a sandboxed frame floating over the space the chat leaves (the frame lives outside #app: the window
// is redrawn from scratch and a frame put back into the page reloads). Pages load at launch, hidden, so
// a tab's count shows before it's opened. A page reaches Cue only by messages, answered here after
// checking what its manifest asked for.
const EXT_ORIGIN = "cueext://localhost";
let extList = [];
const extFrames = new Map();   // name → its iframe, for as long as Cue runs
const extTabOf = new Map();    // session id → the tab it's on (an extension's name); none: Chat
const extBadges = new Map();   // `${name}|${sid}` → the short text on its tab ("4 +98 −9")
const extRan = new Map();      // name → the last command line it ran
const extSent = new Map();     // name → the session info it was last sent (as JSON)

let extRoot = "";
let xtConfirm = null;   // the extension whose "it runs programs" confirmation is showing
const extActivity = new Map();   // name → its last few lines in extensions.log (for Settings)
async function loadExtensions() {
  try { const r = (await invoke("ext_list")) || {}; extList = r.list || []; extRoot = r.root || ""; } catch { extList = []; }
  // An extension switched off (or awaiting approval) loses its page at once; one whose files changed
  // (you copied a new version in, or Cue updated its own) gets a fresh one.
  for (const [n, f] of extFrames) if (!extOf(n)?.enabled || f.dataset.stamp !== (extOf(n).stamp || "")) { f.remove(); extFrames.delete(n); extSent.delete(n); }
  for (const e of extTabs()) extFrame(e);
  await Promise.all(extList.map((e) => invoke("ext_activity", { name: e.name, n: 4 }).then((l) => extActivity.set(e.name, l), () => {})));
  renderMain();
  xtRedraw();
}
const extOf = (name) => extList.find((e) => e.name === name);
const extTabs = () => extList.filter((e) => e.tab && e.enabled);
const extCan = (e, cap) => (e?.capabilities || []).includes(cap);
const openSid = () => quietOpen || active?.sid || null;
const extOn = (sid) => { const n = sid && extTabOf.get(sid); return n && extOf(n)?.enabled ? n : null; };

function extFrame(e) {
  let f = extFrames.get(e.name);
  if (f) return f;
  f = document.createElement("iframe");
  f.className = "ext-frame";
  f.hidden = true;
  f.dataset.stamp = e.stamp || "";
  f.setAttribute("sandbox", "allow-scripts allow-forms");
  f.src = `${EXT_ORIGIN}/${encodeURIComponent(e.name)}/${String(e.entry || "index.html").split("/").map(encodeURIComponent).join("/")}`;
  f.addEventListener("load", () => { extSent.delete(e.name); extNotify(); });
  document.body.append(f);
  extFrames.set(e.name, f);
  return f;
}

/** The row under the activity bar: Chat, then a tab per extension that has one, then Actions (actions.js). */
function sessTabs(sid) {
  const tabs = extTabs();
  if (!sid) return "";
  const on = extOn(sid);
  const ran = on && extRan.get(on);
  return `<div class="stabs"><button class="stab ${on || actTabOn.has(sid) ? "" : "on"}" data-stab="" data-sid="${esc(sid)}">Chat</button>${actTabBtn(sid)}${tabs.map((e) => {
    const b = extBadges.get(`${e.name}|${sid}`);
    return `<button class="stab ${on === e.name ? "on" : ""}" data-stab="${esc(e.name)}" data-sid="${esc(sid)}">${esc(e.tab)}${b ? `<span class="stab-n">${esc(b)}</span>` : ""}</button>`;
  }).join("")}<span class="grow"></span>${ran ? `<span class="stab-ran">ran <code>${esc(ran)}</code></span>` : ""}</div>`;
}
/** Where the tab's page goes, in place of the chat (the chat's element stays, hidden: see placeExt). */
const extSlot = (sid) => (extOn(sid) ? `<div class="ext-slot" data-ext-slot></div>` : "");

/** After each draw: the open session's tab page over its slot, the chat hidden under it; every other frame hidden. */
function placeExt() {
  const sid = openSid(), name = extOn(sid);
  const slot = name && document.querySelector(".active-pane [data-ext-slot]");
  const covered = !!(sheet || lightbox);
  const chat = document.querySelector(".active-pane [data-chat]");
  if (chat) chat.style.display = slot || document.querySelector(".active-pane [data-act-slot]") ? "none" : "";
  for (const [n, f] of extFrames) if (n !== name || !slot || covered) f.hidden = true;
  extNotify();
  if (!slot || covered) return;
  const f = extFrame(extOf(name)), r = slot.getBoundingClientRect();
  Object.assign(f.style, { left: `${r.left}px`, top: `${r.top}px`, width: `${r.width}px`, height: `${r.height}px` });
  f.hidden = false;
}
addEventListener("resize", placeExt);
// Back in Cue (after copying a new version in, say): look again, so a changed extension reloads.
addEventListener("focus", () => { if (extList.length) loadExtensions(); });
document.addEventListener("click", (e) => {
  const t = e.target.closest?.("[data-stab]");
  if (!t) return;
  e.stopPropagation();
  actTabOn.delete(t.dataset.sid);
  if (t.dataset.stab) extTabOf.set(t.dataset.sid, t.dataset.stab); else extTabOf.delete(t.dataset.sid);
  renderMain();
}, true);

// ---------- Extensions panel → Installed ----------
/** What a capability lets an extension do, in plain words. */
function xtCan(cap, programs = []) {
  return {
    "programs.run": `Run these programs installed on your Mac: ${programs.join(", ") || "none listed"}. They can do a lot (a container tool can reach your whole disk): allow only extensions you trust`,
    "sessions.read": "See the open session: its name, folder, machine, and where its turns began",
    "git.read": "Read git in the open session's folders, on this Mac or its machine, without changing anything",
  }[cap] || `Use “${cap}”`;
}
/** A logged line, shown short: "git diff HEAD … · 3m ago". */
function xtLine(l) {
  const [ts, , ...rest] = l.split(" ");
  let text = rest.join(" ").replace(/^says: /, "");
  const where = (text.match(/^\[([^\]:]*):[^\]]*\] /) || [])[1];   // "[build-box:~] …": on that machine
  text = text.replace(/^\[[^\]]*\] /, "").replace(/ --no-optional-locks/, "");
  const code = (text.match(/ -> (\S+)$/) || [])[1];
  text = text.replace(/ -> \S+$/, "");
  const bad = (code && code !== "0") || /^error: /.test(text);
  return `<div class="xt-log-line ${bad ? "bad" : ""}"><code>${esc(text)}${where ? `  · on ${esc(where)}` : ""}${code && code !== "0" ? `  · ${code === "stopped" ? "stopped" : `exit ${esc(code)}`}` : ""}</code><span>${esc(ago(+ts * 1000))}</span></div>`;
}
function xtSettings() {
  const rows = extList.map((e) => {
    const asked = e.capabilities || [], pend = e.pending_capabilities || [], progs = e.programs || [], pendProgs = e.pending_programs || [];
    const newOnly = e.on && (pend.length || pendProgs.length);
    const can = asked.map((c) => `<li>${esc(xtCan(c, progs))}</li>`).join("") || `<li>Nothing beyond showing its page</li>`;
    const more = newOnly ? `<div class="xt-more"><b>Updated, and now also asks to:</b><ul>${[...pend.map((c) => xtCan(c, progs)), ...(pend.length ? [] : pendProgs.map((p) => `Run ${p}`))].map((t) => `<li>${esc(t)}</li>`).join("")}</ul>It's off until you switch it on again.</div>` : "";
    // Switching on one that runs programs asks once more, its programs and what that means in front of you.
    const runs = asked.includes("programs.run") ? progs : [];
    const confirm = xtConfirm === e.name ? `<div class="xt-more xt-confirm"><b>Let ${esc(e.title || e.name)} run ${esc(runs.join(", ") || "programs")} on this Mac?</b>
      These are your own installed programs, and they can do a lot: a container tool can reach your whole disk. It can't run anything it brings with it, a shell, or a script interpreter. Allow it only if you trust it.
      <div class="xt-intro-btns"><button class="btn primary" data-act="xt-toggle" data-name="${esc(e.name)}" data-on="1" data-sure="1">Allow and switch on</button><button class="btn" data-act="xt-cancel">Cancel</button></div></div>` : "";
    const lines = extActivity.get(e.name) || [];
    return `<div class="xt ${e.enabled ? "on" : ""}">
      <div class="xt-head"><span class="xt-name">${esc(e.title || e.name)}</span>${e.version ? `<span class="xt-ver">${esc(e.version)}</span>` : ""}${[e.tab ? `adds the “${esc(e.tab)}” tab to sessions` : "", e.panel ? `in this panel as “${esc(e.panel)}”${e.add ? `, and under New session → + Add` : ""}` : ""].filter(Boolean).length ? `<span class="xt-where">${[e.tab ? `adds the “${esc(e.tab)}” tab to sessions` : "", e.panel ? `in this panel as “${esc(e.panel)}”${e.add ? `, and under New session → + Add` : ""}` : ""].filter(Boolean).join("; ")}</span>` : ""}<span class="grow"></span>
        <span class="xt-state">${e.enabled ? "On" : newOnly ? "Needs your OK" : "Off"}</span>
        <button class="toggle ${e.enabled ? "on" : ""}" data-act="xt-toggle" data-name="${esc(e.name)}" data-on="${e.enabled ? "" : "1"}" role="switch" aria-checked="${!!e.enabled}" aria-label="${esc(e.title || e.name)}"><span></span></button></div>
      <div class="xt-body"><div class="xt-can"><div class="xt-k">${e.enabled ? "It can" : "Switching it on lets it"}</div><ul>${can}</ul></div>${more}${confirm}
        <div class="xt-log"><div class="xt-k">Last it did</div>${lines.length ? lines.map(xtLine).join("") : `<div class="xt-none">Nothing yet</div>`}</div>
        <div class="xt-folder">${e.builtin ? "Comes with Cue, and updates with it" : `${e.replaces_builtin ? "Your copy, used in place of the one that comes with Cue: " : ""}<code>${esc(e.folder || "")}</code>`}</div></div></div>`;
  }).join("");
  return `<div class="xt-intro">An extension is a folder with a <code>manifest.json</code> and its own page, in <code>${esc(extRoot)}</code>. Nothing in it runs until you switch it on here, and switching it on is what allows what it asks for. Every command it runs is listed under it.
      <div class="xt-intro-btns"><button class="btn" data-act="xt-folder">Open the folder</button><button class="btn" data-act="xt-rescan">Look again</button></div></div>
    ${rows || `<div class="xt-empty">No extensions yet. Put one's folder in the folder above, then Look again.</div>`}`;
}
document.addEventListener("click", async (e) => {
  const t = e.target.closest?.("[data-act]");
  const act = t?.dataset.act;
  if (!["xt-toggle", "xt-folder", "xt-rescan", "xt-cancel"].includes(act)) return;
  e.stopPropagation();
  if (act === "xt-folder") return invoke("ext_reveal").catch((x) => toast(`Couldn't open it: ${x}`));
  if (act === "xt-rescan") return loadExtensions();
  if (act === "xt-cancel") { xtConfirm = null; return xtRedraw(); }
  // Switching on one that runs programs: first the confirmation, then (from its button) for real.
  const ext = extOf(t.dataset.name);
  if (act === "xt-toggle" && t.dataset.on && !t.dataset.sure && (ext?.capabilities || []).includes("programs.run")) { xtConfirm = t.dataset.name; return xtRedraw(); }
  xtConfirm = null;
  try { await invoke("ext_enable", { name: t.dataset.name, on: !!t.dataset.on }); } catch (x) { toast(`Couldn't: ${x}`); }
  loadExtensions();
}, true);

const EXT_VARS = ["bg", "on-bg", "on-bg-muted", "panel", "panel-2", "line", "ink", "muted", "accent", "on-accent", "sel", "on-sel", "work", "deny", "star", "shadow", "sans", "mono"];
function extTheme() {
  const cs = getComputedStyle(document.documentElement);
  const set = document.documentElement.dataset.theme;
  const scheme = set === "dark" || set === "light" ? set : matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  return { scheme, vars: Object.fromEntries(EXT_VARS.map((k) => [k, cs.getPropertyValue(`--${k}`).trim()])) };
}
const fromBase = (b) => (b ? { root: b.root, commit: b.commit, untracked: b.untracked || [] } : null);
/** What a page is told about a session: never more than it needs to show its tab. */
function extSessionInfo(sid) {
  const s = sid && sessionOf(sid), q = !s && sid && (state.live || []).find((x) => x.session_id === sid);
  if (!s && !q) return null;
  const x = s || q;
  return {
    id: sid, name: nameOf(sid, x.project), folder: x.cwd || "", machine: x.machine || "",
    state: s ? s.state : q.status || "", agent: x.harness || "",
    turn_from: fromBase(s?.turn_from), session_from: fromBase(s?.session_from),
  };
}
function extSend(name, msg, frame) { (frame || extFrames.get(name))?.contentWindow?.postMessage({ cue: 1, ...msg }, "*"); }
/** Each page hears the open session (when it changes, or anything about it does) and the theme. */
let extThemeSent = "";
function extNotify() {
  const t = extTheme(), tk = JSON.stringify(t), fresh = tk !== extThemeSent;
  extThemeSent = tk;
  const info = extSessionInfo(openSid()), ik = JSON.stringify(info);
  for (const [name] of extFrames) {
    if (fresh) extSend(name, { event: "theme", data: t });
    if (extCan(extOf(name), "sessions.read") && extSent.get(name) !== ik) { extSent.set(name, ik); extSend(name, { event: "session", data: info }); }
  }
}

// ---------- the Extensions panel ----------
// Where an extension's page goes when it isn't about one chat: an extension whose manifest has a "panel"
// (its label) is listed in the panel the Extensions button in the top bar opens, beside the terminal's.
// One with an "add" too (a label) is also offered under New session → + Add, opening the panel on it.
// Its page is made when shown and gone when the panel closes (nothing in the background), and speaks
// to Cue just as a tab's page does, with the same checks.
let extAddMenu = false;
let extPanel = null;   // { name, frame, el } while the panel is open
const extPanels = () => extList.filter((e) => e.panel && e.enabled);
const EXT_ICON = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M19.439 7.85c-.049.322.059.648.289.878l1.568 1.568c.47.47.706 1.087.706 1.704s-.235 1.233-.706 1.704l-1.611 1.611a.98.98 0 0 1-.837.276c-.47-.07-.802-.48-.968-.925a2.501 2.501 0 1 0-3.214 3.214c.446.166.855.497.925.968a.979.979 0 0 1-.276.837l-1.61 1.61a2.404 2.404 0 0 1-1.705.707 2.402 2.402 0 0 1-1.704-.706l-1.568-1.568a1.026 1.026 0 0 0-.877-.29c-.493.074-.84.504-1.02.968a2.5 2.5 0 1 1-3.237-3.237c.464-.18.894-.527.967-1.02a1.026 1.026 0 0 0-.289-.877l-1.568-1.568A2.402 2.402 0 0 1 1.998 12c0-.617.236-1.234.706-1.704L4.23 8.77c.24-.24.581-.353.917-.303.515.077.877.528 1.073 1.01a2.5 2.5 0 1 0 3.259-3.259c-.482-.196-.933-.558-1.01-1.073-.05-.336.062-.676.303-.917l1.525-1.525A2.402 2.402 0 0 1 12 1.998c.617 0 1.234.236 1.704.706l1.568 1.568c.23.23.556.338.877.29.493-.074.84-.504 1.02-.968a2.5 2.5 0 1 1 3.237 3.237c-.464.18-.894.527-.967 1.02Z"/></svg>`;
const extPanelBtn = () => `<button class="top-btn icon xp-btn" data-xp-open="" aria-pressed="${!!extPanel}" aria-label="Extensions">${EXT_ICON}</button>`;
const extAddMenuHtml = () => (extAddMenu ? `<div class="xw-menu"><button data-xw=""><b>A machine you reach over SSH</b><span>build box, cloud VM…</span></button>${extPanels().filter((e) => e.add).map((e) => `<button data-xw="${esc(e.name)}"><b>${esc(e.add)}</b><span>${esc(e.title || e.name)}</span></button>`).join("")}</div>` : "");
const extAddsAny = () => extPanels().some((e) => e.add);

/** The panel's first entry, "Installed": every extension, switched on or off, what it may do, its last
 *  commands (Cue's own page, drawn here, not an extension's). */
function xtRedraw() {
  const pane = extPanel && extPanel.name === "" && extPanel.el.querySelector(".xp-manage");
  if (pane) pane.innerHTML = xtSettings();
}
function extShowPanel(name) {
  const list = extPanels();
  const e = name && extOf(name)?.panel && extOf(name).enabled ? extOf(name) : null;
  if (!extPanel) {
    const el = document.createElement("div");
    el.className = "scrim xp-scrim";
    el.innerHTML = `<div class="sheet xp"><div class="sheet-head"><h3>Extensions</h3><span class="grow"></span><button class="xw-x" data-xp-close aria-label="Close">×</button></div><div class="xp-body"><nav class="xp-rail"></nav><div class="xp-page"></div></div></div>`;
    el.addEventListener("click", (ev) => {
      if (ev.target === el || ev.target.closest("[data-xp-close]")) return extClosePanel();
      const r = ev.target.closest("[data-xp]");
      if (r) extShowPanel(r.dataset.xp || "");
    });
    document.body.append(el);
    extPanel = { name: null, frame: null, el };
  }
  extPanel.el.querySelector(".xp-rail").innerHTML = `<button class="${e ? "" : "on"}" data-xp="">Installed</button>${list.length ? `<div class="xp-sep"></div>` : ""}` + list.map((x) => `<button class="${x.name === e?.name ? "on" : ""}" data-xp="${esc(x.name)}">${esc(x.panel)}</button>`).join("");
  const want = e ? e.name : "";
  if (extPanel.name === want) return xtRedraw();
  extPanel.frame?.remove();
  extPanel.frame = null;
  extPanel.el.querySelector(".xp-manage")?.remove();
  if (!e) {
    const m = document.createElement("div");
    m.className = "xp-manage";
    extPanel.el.querySelector(".xp-page").append(m);
    extPanel.name = "";
    xtRedraw();
    renderMain();
    loadExtensions();
    return;
  }
  const f = document.createElement("iframe");
  f.className = "xp-frame";
  f.setAttribute("sandbox", "allow-scripts allow-forms");
  f.src = `${EXT_ORIGIN}/${encodeURIComponent(e.name)}/${String(e.entry || "index.html").split("/").map(encodeURIComponent).join("/")}`;
  f.addEventListener("load", () => { extSend(e.name, { event: "theme", data: extTheme() }, f); if (extCan(e, "sessions.read")) extSend(e.name, { event: "session", data: extSessionInfo(openSid()) }, f); });
  extPanel.el.querySelector(".xp-page").append(f);
  extPanel.name = e.name;
  extPanel.frame = f;
  renderMain();
}
function extClosePanel() { if (extPanel) { extPanel.el.remove(); extPanel = null; renderMain(); } }
document.addEventListener("keydown", (ev) => { if (ev.key === "Escape" && extPanel) { ev.stopPropagation(); extClosePanel(); } }, true);
document.addEventListener("click", (ev) => {
  const open = ev.target.closest?.("[data-xp-open]");
  if (open) { ev.stopPropagation(); return extPanel ? extClosePanel() : extShowPanel(open.dataset.xpOpen); }
  const t = ev.target.closest?.("[data-xw]");
  if (!t) return;
  ev.stopPropagation();
  extAddMenu = false;
  if (!t.dataset.xw) { nfAdding = true; renderMain(); return document.querySelector('[data-text="mc-host"]')?.focus(); }
  renderMain();
  extShowPanel(t.dataset.xw);
}, true);

addEventListener("message", async (e) => {
  const name = [...extFrames].find(([, f]) => f.contentWindow === e.source)?.[0] || (extPanel?.frame && extPanel.frame.contentWindow === e.source ? extPanel.name : null);
  const m = e.data;
  if (!name || !m || m.cue !== 1 || m.id == null) return;
  const ext = extOf(name), p = m.params || {};
  const reply = (ok, value) => e.source.postMessage({ cue: 1, id: m.id, ok, value }, "*");
  const need = (cap) => { if (!extCan(ext, cap)) throw new Error(`${name} didn't ask for ${cap}`); };
  try {
    switch (m.method) {
      case "hello": return reply(true, { name, theme: extTheme() });
      case "session.current": need("sessions.read"); return reply(true, extSessionInfo(openSid()));
      case "programs.run": {
        need("programs.run");
        const r = await invoke("ext_program", { ext: name, program: String(p.program || ""), args: (p.args || []).map(String), cwd: p.cwd ? String(p.cwd) : null });
        const ran = `${p.program} ${(p.args || []).join(" ")}`.trim();
        if (extRan.get(name) !== ran) { extRan.set(name, ran); renderMain(); }
        return reply(true, r);
      }
      case "git": {
        // Read-only git that Cue runs, in the open session's folder (Cue's record of it, not the page's word).
        need("git.read");
        const sid = String(p.session || "");
        if (sid !== openSid()) throw new Error("only the open session");
        const x = sessionOf(sid) || (state.live || []).find((q) => q.session_id === sid);
        if (!x) throw new Error("no such session");
        const r = await invoke("ext_git", { ext: name, op: String(p.op || ""), folder: x.cwd || "", machine: x.machine || null,
          against: p.against ? String(p.against) : null, path: p.path ? String(p.path) : null, transcript: x.transcript_path || null });
        const ran = `git ${p.op}${p.against ? ` ${String(p.against).slice(0, 9)}` : ""}${p.path ? ` ${p.path}` : ""}${x.machine ? ` · on ${x.machine}` : ""}`;
        if (extRan.get(name) !== ran) { extRan.set(name, ran); renderMain(); }
        return reply(true, r);
      }
      case "tab.badge": {
        const k = `${name}|${String(p.session || "")}`, v = String(p.text || "").slice(0, 24);
        if (extBadges.get(k) !== v) { if (v) extBadges.set(k, v); else extBadges.delete(k); renderMain(); }
        return reply(true, null);
      }
      case "log": await invoke("ext_log", { ext: name, text: String(p.text || "") }); return reply(true, null);
      case "ui.toast": toast(`${ext.title || name}: ${String(p.text || "").slice(0, 200)}`); return reply(true, null);
      default: throw new Error(`no such call: ${m.method}`);
    }
  } catch (err) { reply(false, String(err?.message || err)); }
});
