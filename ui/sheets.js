// Cue UI sheets — Settings, Search, the conversation sheet, the setup sheet, Forward and the move dialog.
// "Move to Cue" in progress: a dialog over the window says what's happening, step by step, and the screen
// behind it stays as it is (the session vanishes from the lists while it's ended and resumed). When it's
// done the moved session is the selected one; its terminal stays closed until you open it.
let moving = null;   // { sid, name, harness, project, step, err, at, last }
function movingDialog() {
  const m = moving;
  const steps = ["Ending it in its tab", "Resuming it in Cue: loading the conversation", "Ready"];
  const secs = Math.round((now() - m.at) / 1000);
  return `<div class="move-scrim"><div class="move-dialog" role="dialog" aria-live="polite">${m.err
    ? `<div class="move-title">Couldn't move ${esc(m.name || m.project)} to Cue</div><div class="move-err">${esc(m.err)}</div><div class="row-btns"><span class="grow"></span><button class="btn primary" data-act="move-dismiss">OK</button></div>`
    : `<div class="move-title"><span class="dot-live"></span>Moving ${esc(m.name || m.project)} to Cue…<span class="grow"></span><span class="move-secs">${secs}s</span></div><ol class="move-steps">${steps.map((t, n) => `<li class="${n < m.step ? "done" : n === m.step ? "now" : ""}">${t}</li>`).join("")}</ol>${m.step === 1 && m.last ? `<div class="move-screen">${esc(m.last)}</div>` : ""}<div class="dim">${m.step === 1 ? "A long conversation takes a while to load." : "Its conversation comes back whole."}</div>`}
  </div></div>`;
}
async function moveToCue(sid) {
  // A quiet one (Cue hasn't heard from it yet) is only in the live list: its name and folder from there.
  const q = (state.live || []).find((x) => x.session_id === sid);
  const s = sessionOf(sid) || (q && { name: bareName(q.name) || baseName(q.cwd), harness: q.harness, project: projOf(q.cwd) });
  if (!s || moving) return;
  if (quietOpen === sid) quietOpen = null;   // it opens in Active as it moves
  moving = { sid, name: s.name, harness: s.harness, project: s.project, step: 0, err: null, at: now(), last: "" };
  renderMain();
  // Every second: the clock, and once it's in its new window, that window's last line.
  const watch = setInterval(async () => {
    if (!moving || moving.sid !== sid) return clearInterval(watch);
    if (inTmux(sid)) {
      moving.step = Math.max(moving.step, 1);
      try { const sc = await invoke("session_screen", { sessionId: sid }); moving.last = (sc.split("\n").map((l) => l.trim()).filter(Boolean).at(-1) || "").slice(0, 120); } catch {}
    }
    renderMain();
  }, 1000);
  try {
    await invoke("move_to_cue", { sessionId: sid });
    clearInterval(watch);
    moving.step = 2; renderMain();
    for (let n = 0; n < 40 && !inTmux(sid); n++) await new Promise((r) => setTimeout(r, 100));
    moving = null;
    if (sessionOf(sid)) setActive(null, sid); else renderMain();
  } catch (e) {
    clearInterval(watch);
    moving.err = String(e);
    renderMain();
  }
}
let searchVisible = [];        // result indexes in screen order (↓ / ↑ walk these)
let searchLive = [], searchFound = [];   // running sessions (matched here) and what Cue's log found (asked for)
const searchMore = new Set();  // sessions whose every match you asked to see
/** How well a name matches what you typed, best first: 0 the whole name, 1 its start, 2 the start of a
 *  word in it, 3 anywhere in it, 4 every word you typed somewhere in it; null: it doesn't. */
function nameScore(name, q) {
  const n = String(name || "").toLowerCase();
  if (!q || !n) return null;
  if (n === q) return 0;
  if (n.startsWith(q)) return 1;
  if (n.split(/[\s\-_./:]+/).some((w) => w.startsWith(q))) return 2;
  if (n.includes(q)) return 3;
  return q.split(/\s+/).every((w) => n.includes(w)) ? 4 : null;
}
/** Running sessions matching `q`, as ⌘K matches them: by name first, then by folder or what they're on. */
function liveMatches(q) {
  q = q.trim().toLowerCase();
  if (!q) return [];
  return liveRows().map((r) => {
    const ab = svAbout(r), unnamed = r.name === baseName(r.cwd) && (ab.about || ab.now);
    const name = unnamed ? ab.about || ab.now : r.name;
    const byName = Math.min(...[nameScore(name, q), nameScore(r.name, q)].map((x) => x ?? 9));
    const rest = [r.cwd, ab.about, ab.now, (r.crew || crewOf(r.sid))?.lead_name].join(" ").toLowerCase();
    const score = byName < 9 ? byName : q.split(/\s+/).every((w) => rest.includes(w)) ? 5 : null;
    return score === null ? null : { kind: "session", live: true, quiet: r.quiet, session_id: r.sid, name, project: r.project || projOf(r.cwd), harness: r.harness, at_ms: r.since, last: r.what || ab.now || "", score, rank: svRank(r) };
  }).filter(Boolean).sort((a, b) => a.score - b.score || a.rank - b.rank || b.at_ms - a.at_ms);
}
/** The results in the order they're shown: running sessions, closed ones (best name match first), older
 *  ones, then what was said, grouped by session (the session with the newest match first). */
function combineSearch(q) {
  q = q.trim().toLowerCase();
  const live = new Set(searchLive.map((r) => r.session_id));
  const byName = (a, b) => (nameScore(a.name, q) ?? nameScore(a.project, q) ?? 9) - (nameScore(b.name, q) ?? nameScore(b.project, q) ?? 9) || b.at_ms - a.at_ms;
  const closed = searchFound.filter((x) => x.kind === "session" && !live.has(x.session_id) && !sessionOf(x.session_id)).sort(byName);
  const older = searchFound.filter((x) => x.kind === "older").sort(byName);
  const msgs = searchFound.filter((x) => x.kind === "message");
  const order = [...new Set(msgs.map((m) => m.session_id))];
  searchResults = [...searchLive, ...closed, ...older, ...order.flatMap((sid) => msgs.filter((m) => m.session_id === sid))];
}
function runSearch(q) {
  // Running sessions: at once, as you type.
  searchLive = liveMatches(q);
  if (!q.trim()) searchFound = [];
  combineSearch(q);
  searchMore.clear();
  searchSel = 0;
  renderMain();
  // Everything else Cue has logged: once you pause.
  clearTimeout(searchTimer);
  searchTimer = setTimeout(async () => {
    const n = ++searchSeq;
    const r = q.trim() ? await invoke("search", { q }).catch(() => []) : [];
    if (n !== searchSeq) return;   // a newer search already started
    searchFound = Array.isArray(r) ? r : [];
    const sel = searchResults[searchSel];
    combineSearch(q);
    // What you'd picked stays picked when the rest arrives.
    searchSel = Math.max(0, searchResults.findIndex((x) => x === sel));
    renderMain();
  }, 140);
}
function openSearch() {
  sheet = "search";
  // Text left from last time: search it again, so running sessions are as they are now.
  if (draft("search").text.trim()) return runSearch(draft("search").text), focusSearch();
  renderMain();
  focusSearch();
}
function focusSearch() {
  const el = document.querySelector(".search-in");
  el?.focus(); el?.select();
}
function searchSheet() {
  const q = draft("search").text.trim();
  const mark = (html) => q ? html.replace(new RegExp(esc(q).replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"), (m) => `<mark>${m}</mark>`) : html;
  searchVisible = [];
  const row = (r, i, inGroup = false) => {
    searchVisible.push(i);
    const top = inGroup ? `<span class="dim">${r.role === "you" ? "you said" : "agent"}</span><span class="grow"></span><span class="age">${ago(r.at_ms)}</span>` : `${badge(r.harness)}<span class="proj">${mark(esc(r.name || r.project || "session"))}</span>${r.name && r.project && r.name !== r.project ? `<span class="dim">${esc(r.project)}</span>` : ""}
      <span class="dim">${r.kind === "older" ? "" : r.kind === "session" ? sessionState(r) : r.role === "you" ? "you said" : "agent"}</span>${r.kind === "older" ? `<span class="sr-resume">Resume</span>` : r.kind === "session" && !r.live && r.harness === "claude" ? `<span class="sr-resume act" role="button" data-resume="${esc(r.session_id)}" data-name="${esc(r.name || r.project || "")}">Resume</span>` : ""}${r.ended && r.kind === "message" ? `<span class="sr-ended">ended</span>` : ""}<span class="grow"></span><span class="age">${ago(r.at_ms)}</span>`;
    const body = r.kind === "message" ? `<div class="sr-snip">${mark(esc(r.snippet))}</div>`
      : r.last ? `<div class="sr-snip sr-last">${r.last_role === "you" ? "<b>You:</b> " : ""}${mark(esc(r.last))}</div>` : "";
    // Under its session's name, one line is enough: who, what, when.
    if (inGroup) return `<button class="sr in-group ${searchSel === i ? "sel" : ""}" data-sr="${i}"><span class="sr-who">${r.role === "you" ? "you" : "agent"}</span><span class="sr-line">${mark(esc(r.snippet))}</span><span class="age">${ago(r.at_ms)}</span></button>`;
    return `<button class="sr ${searchSel === i ? "sel" : ""}" data-sr="${i}"><div class="sr-top">${top}</div>${body}</button>`;
  };
  // Sections, so it's clear what each result is: running now, ended (Cue saw it), older (from Claude
  // Code's own archive, before Cue: Resume opens it again), and what was said.
  const section = (title, note, keep) => {
    const rows = searchResults.map((r, i) => [r, i]).filter(([r]) => keep(r));
    return rows.length ? `<div class="sr-sec">${title.toUpperCase()} <span>${rows.length}</span><em>${note}</em></div>${rows.map(([r, i]) => row(r, i)).join("")}` : "";
  };
  // History, grouped by session: its name once, then its matches (the 3 newest, until you ask for more).
  const SHOWN = 3;
  const history = () => {
    const groups = [];
    searchResults.forEach((r, i) => {
      if (r.kind !== "message") return;
      let g = groups.find((x) => x.sid === r.session_id);
      if (!g) groups.push((g = { sid: r.session_id, r, rows: [] }));
      g.rows.push([r, i]);
    });
    if (!groups.length) return "";
    const total = groups.reduce((n, g) => n + g.rows.length, 0);
    return `<div class="sr-sec">HISTORY <span>${total}</span><em>messages and answers</em></div>` + groups.map((g) => {
      const all = searchMore.has(g.sid) || g.rows.length <= SHOWN + 1, name = esc(g.r.name || g.r.project || "session");
      const head = `<div class="sr-group">${badge(g.r.harness)}<span class="proj">${name}</span>${g.r.ended ? `<span class="sr-ended">ended</span>` : ""}<span class="dim">${g.rows.length} ${g.rows.length === 1 ? "match" : "matches"}</span></div>`;
      return head + (all ? g.rows : g.rows.slice(0, SHOWN)).map(([r, i]) => row(r, i, true)).join("")
        + (all ? "" : `<button class="sr-more" data-srmore="${esc(g.sid)}">+ ${g.rows.length - SHOWN} more in ${name}</button>`);
    }).join("");
  };
  const list = !q ? `<div class="dim sr-hint">Live and closed sessions, older Claude Code sessions by name (Resume them), and everything said in sessions Cue has seen.</div>`
    : searchResults.length
      ? section("Live", "running now", (r) => r.kind === "session" && r.live)
        + section("Closed", "ended; Cue has the conversation", (r) => r.kind === "session" && !r.live)
        + section("Older", "from Claude Code, before Cue; matched by name", (r) => r.kind === "older")
        + history()
      : `<div class="dim sr-hint">Nothing found for “${esc(q)}”.</div>`;
  return `<div class="sheet search-sheet"><div class="search-bar">${SEARCH_ICON}<input class="search-in" data-text="search" placeholder="Search sessions (live, closed, older) and messages…" spellcheck="false" autocomplete="off" value="${esc(draft("search").text)}"/><kbd>Esc</kbd></div>
    <div class="sheet-body">${list}</div></div>`;
}
/** ↓ / ↑ in the search box: move the highlight (Enter opens it), keeping it in view. */
function stepSearch(by) {
  if (!searchVisible.length) return;
  const at = Math.max(0, searchVisible.indexOf(searchSel));
  searchSel = searchVisible[(at + by + searchVisible.length) % searchVisible.length];
  renderMain();
  document.querySelector(`[data-sr="${searchSel}"]`)?.scrollIntoView({ block: "nearest" });
}
/** What a session result is up to: live state, or ended. */
function sessionState(r) {
  const s = sessionOf(r.session_id);
  if (!s) return "";   // ended: its section already says so
  if (state.items.some((i) => i.session_id === r.session_id)) return "your turn";
  return s.state === "working" ? "working" : "idle";
}
/** A result: a live session opens in Active; a message (or an ended session) opens its whole
 *  conversation, scrolled to that message. */
function pickResult(i) {
  const r = searchResults[i];
  if (!r) return;
  if (r.kind === "session" && r.live && r.quiet) { sheet = null; return svAct("quiet", r.session_id); }
  if (r.kind === "session" && sessionOf(r.session_id)) { sheet = null; return openSession(r.session_id); }
  if (r.kind === "older") return resumeOlder(r);
  openConvo(r, r.kind === "message" ? r.at_ms : null);
}
/** An older Claude Code session: `claude --resume` in a terminal tab behind Cue, and the session
 *  opens here in Active (with its last few messages), ready to type into. */
async function resumeOlder(r) {
  sheet = null;
  renderMain();
  try {
    const res = await invoke("resume_session", { sessionId: r.session_id });
    toast(`${res.detail}: ${r.name || r.project}`);
    setActive(null, res.session_id);
  } catch (e) { toast(`Couldn't resume it: ${e}`); }
}
let convo = null, convoScrolled = false;  // the conversation opened from search, and whether it's been scrolled to its hit
async function openConvo(r, hitAt) {
  const items = await invoke("session_log", { sessionId: r.session_id }).catch(() => []);
  convo = { sid: r.session_id, hitAt, name: r.name || r.project, harness: r.harness, items: Array.isArray(items) ? items : [] };
  sheet = "convo";
  convoScrolled = false;
  renderMain();
}
/** A session's whole conversation (from Cue's log), read-only, with the message you found in lime. */
function convoSheet() {
  const c = convo, live = sessionOf(c.sid);
  const when = (ms) => new Date(ms).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
  const msgs = c.items.map((e) => {
    const hit = e.at_ms === c.hitAt ? " cv-hit" : "";
    if (e.role === "you") return `<div class="cv-you${hit}"><div class="cv-you-text">${linkify(esc(e.text)).replace(/\n/g, "<br>")}</div>${thumbs(e.images)}<div class="cv-meta">You · ${when(e.at_ms)}</div></div>`;
    return `<div class="cv-agent${hit}"><div class="cv-meta">${esc(e.role === "peer" ? "Another session" : agentName(c.harness))}${e.role === "peer" ? "" : machTag(live?.machine)} · ${when(e.at_ms)}</div><div class="msg cv-text">${md(e.text)}</div></div>`;
  }).join("");
  return `<div class="sheet convo-sheet"><div class="sheet-head"><button class="btn" data-act="convo-back">← Results</button>${badge(c.harness)}<h3>${esc(c.name)}</h3>
      <span class="dim" style="font-size:12px">${live ? (live.state === "working" ? "working" : "live") : "ended"} · ${c.items.length} messages</span><span class="grow"></span>
      ${live ? `<button class="btn primary" data-act="convo-open" data-sid="${esc(c.sid)}">Open in Active</button>` : c.harness === "claude" ? `<button class="btn primary" data-act="convo-resume">Resume</button>` : ""}<kbd>Esc</kbd></div>
    <div class="sheet-body convo-body">${msgs || `<div class="dim">Nothing logged for this session.</div>`}</div></div>`;
}
/** "Send to another session": a small menu right above the button. Pick a session, add a note if you like. */
function forwardPop() {
  const f = forward;
  const card = new Map(state.items.map((i) => [i.session_id, i]));
  const label = (x) => card.has(x.session_id) ? (card.get(x.session_id).kind === "waiting" ? "your turn" : "asks you") : idleNote(x) ? "idle" : "working";
  const opts = state.sessions.filter((x) => x.session_id !== f.fromSid).map((x) => `<button class="fwd-opt ${f.to === x.session_id ? "on" : ""}" data-fwd-to="${esc(x.session_id)}">
      ${badge(x.harness)}<span class="proj">${esc(x.name || x.project)}</span><span class="grow"></span><span class="fwd-st">${esc(label(x))}</span></button>`).join("");
  const to = sessionOf(f.to);
  return `<div class="fwd-catch" data-act="cancel-forward"></div><div class="fwd-pop" role="dialog" aria-label="Send to another session">
    <div class="fwd-head">SEND THIS MESSAGE TO</div>
    <div class="fwd-list">${opts || `<div class="dim" style="padding:6px 8px">No other sessions are open.</div>`}</div>
    <textarea rows="2" class="fwd-note" spellcheck="false" data-text="fwd" placeholder="Add a note (optional)">${esc(draft("fwd").text)}</textarea>
    <div class="fwd-foot"><button class="btn" data-act="cancel-forward">Cancel</button><button class="btn primary" data-act="send-forward" ${to ? "" : "disabled"}>${to ? `Send to ${esc(to.project)}` : "Pick a session"}</button></div></div>`;
}
/** Keep the menu just above its button (below it when there's no room), following the chat as it scrolls. */
function placeForward() {
  const pop = document.querySelector(".fwd-pop");
  if (!pop) return;
  const btn = document.querySelector(`[data-fwd="${CSS.escape(forward.key)}"]`);
  if (!btn) { sheet = forward = null; return renderMain(); }
  const r = btn.getBoundingClientRect(), h = pop.offsetHeight, w = pop.offsetWidth;
  pop.style.left = `${Math.max(8, Math.min(r.left, innerWidth - w - 8))}px`;
  pop.style.top = `${r.top - h - 6 >= 8 ? r.top - h - 6 : Math.min(r.bottom + 6, innerHeight - h - 8)}px`;
}
async function sendForward() {
  const f = forward, to = sessionOf(f?.to);
  if (!to) return;
  if (to.state === "deciding") return toast(`${to.project} is asking you something first. Answer that, then send.`);
  const note = draft("fwd").text.trim();
  const text = `${note ? `${note}\n\n` : ""}[Forwarded from the ${f.from} session]\n${f.text}`;
  sheet = forward = null;
  delete drafts.fwd;
  renderMain();
  try { await invoke("send_to_session", { sessionId: to.session_id, text, images: [] }); toast(`Sent to ${to.project}`); }
  catch (e) { toast(`Couldn't send to ${to.project}: ${e}`); }
}
const seg = (key, cur, opts) => `<div class="seg">${opts.map(([v, l]) => `<button class="${JSON.stringify(v) === JSON.stringify(cur) ? "on" : ""}" data-set="${key}" data-val='${esc(JSON.stringify(v))}'>${esc(l)}</button>`).join("")}</div>`;
const toggle = (key, on) => `<button class="toggle ${on ? "on" : ""}" data-set="${key}" data-val="${!on}" role="switch" aria-checked="${on}" aria-label="${esc(key)}"><span></span></button>`;
const setRow = (title, sub, control) => `<div class="set-row"><div class="set-text"><div class="set-title">${title}</div>${sub ? `<div class="set-sub">${sub}</div>` : ""}</div>${control}</div>`;
/** Settings rows an add-on adds (fetched when Settings opens; none without one). */
let extSections = null;
function extRows() {
  if (!extSections) { extSections = []; invoke("ext_settings").then((v) => { extSections = v || []; renderMain(); }, () => {}); }
  return extSections.map((g) => `<div class="set-group">${esc(g.group)}</div>${(g.rows || []).map((r) =>
    `<div class="set-row ${r.art ? "with-art" : ""}"><div class="set-text"><div class="set-title">${esc(r.title)}</div>${r.sub ? `<div class="set-sub">${r.sub}</div>` : ""}</div>${r.art ? `<div class="set-art">${r.art}</div>` : ""}</div>`).join("")}`).join("");
}
/** Updates: what Cue found (from GitHub's latest release, checked by Cue itself). `offer`: a newer
 *  version found in the background, shown as a card until you update or put it off. */
let upd = { current: "", version: "", notes: "", status: "", offer: false };
async function checkUpdate() {
  upd = { ...upd, status: "checking" };
  renderMain();
  try { const r = await invoke("update_check"); upd = { ...upd, current: r.current, version: r.version || "", notes: r.notes || "", status: r.version ? "found" : "latest" }; }
  catch (e) { upd = { ...upd, status: `error:${e}` }; }
  renderMain();
}
async function installUpdate() {
  upd = { ...upd, status: "installing" };
  renderMain();
  try { await invoke("update_install"); } catch (e) { upd = { ...upd, status: `error:${e}` }; renderMain(); }
}
function updateRow() {
  if (!upd.status) checkUpdate();   // the first time Settings opens
  const st = upd.status;
  const sub = st === "checking" ? "Checking…" : st === "installing" ? `Installing Cue ${esc(upd.version)}. Cue restarts by itself when it's done.`
    : st === "found" ? `Cue ${esc(upd.version)} is out.${upd.notes ? ` ${esc(upd.notes.split("\n")[0].slice(0, 140))}` : ""}`
    : st === "latest" ? "You have the latest version." : st.startsWith("error:") ? esc(st.slice(6)) : "";
  const btn = st === "found" ? `<button class="btn primary" data-upd="install">Update and restart</button>`
    : `<button class="btn" data-upd="check" ${st === "checking" || st === "installing" ? "disabled" : ""}>Check for updates</button>`;
  return setRow(`Cue ${esc(upd.current || "")}`, `${sub}${sub ? " " : ""}Cue also looks by itself a minute after it opens, then every 6 hours.`, btn);
}
/** The agents Cue connects to, as Settings rows (also the first-launch setup screen's). */
function agentRows() {
  const c = state.connections || {};
  const conn = (ok) => `<span class="conn ${ok ? "ok" : ""}"><i></i>${ok ? "connected" : "not connected"}</span>`;
  // Not connected but installed: one click does what install.sh does.
  const connBtn = (h, ok, present) => (ok || !present ? conn(ok) : `<button class="btn primary" data-connect="${h}">Connect</button>`);
  return `${setRow("Claude Code", c.claude?.ok ? "Sessions already open when you connected: type <code>/hooks</code> in each once (or restart it with claude --resume)." : c.claude?.present === false ? "Not installed on this Mac." : "Not connected yet. Connect adds Cue's hooks to <code>~/.claude/settings.json</code> (backed up first; your other settings stay).", connBtn("claude", c.claude?.ok, c.claude?.present !== false))}
    ${setRow("Codex", c.codex?.ok ? "Hooks installed. In Codex, run <code>/hooks</code> once to trust them." : c.codex?.present ? "Not connected yet. Connect adds Cue's hooks to <code>~/.codex/hooks.json</code> (backed up first)." : "Not installed on this Mac.", connBtn("codex", c.codex?.ok, c.codex?.present))}
    ${setRow("Pi", c.pi?.ok ? "New Pi sessions load the Cue extension. Replies go straight into the session." : c.pi?.present ? "Not connected yet. Connect adds Cue's extension to Pi." : "Not installed on this Mac.", connBtn("pi", c.pi?.ok, c.pi?.present))}`;
}
/** First launch: connect the agents you use (until Done). */
function setupSheet() {
  const todo = ["claude", "codex", "pi"].filter((h) => { const c = state.connections?.[h]; return c && !c.ok && c.present; });
  return `<div class="sheet setup-sheet"><div class="sheet-head"><h3>Connect your agents</h3></div><div class="sheet-body">
    <p class="setup-lede">Cue shows you what your coding agents need from you: a finished turn, a permission, a question. Connect the ones you use. You can change this later in Settings.</p>
    ${agentRows()}
    <div class="setup-foot">${todo.length > 1 ? `<button class="btn" data-connect-all>Connect all</button>` : ""}<button class="btn primary" data-setup-done>Done</button></div>
  </div></div>`;
}
// ---------- + New → Machine: other machines to run sessions on, over SSH ----------
let machinesList = null;            // null until asked
const machineState = new Map();     // name -> what its check found, or "checking"
const TOOL_NAMES = { claude: "Claude Code", codex: "Codex", pi: "Pi", tmux: "tmux" };
function loadMachines() {
  machinesList = machinesList || [];
  invoke("machines_list").then((l) => {
    machinesList = l;
    for (const m of l) if (!machineState.has(m.name)) checkMachine(m.name);
    renderMain();
  }).catch(() => {});
}
function checkMachine(name) {
  machineState.set(name, "checking");
  renderMain();
  return invoke("machine_check", { name }).then((r) => machineState.set(name, r), (e) => machineState.set(name, { ok: false, error: String(e) })).then(() => {
    const st = machineState.get(name);
    // Connected, with Claude Code or Codex there: Cue's hooks for them (only those; it installs nothing).
    if (st?.ok && (st.tools?.claude || st.tools?.codex)) setupHooks(name, st.tools);
    renderMain();
    return st;
  });
}
/** What setting up Cue's hooks on a machine did (shown under it): lines, or an error. */
const machineHooks = new Map();
async function setupHooks(name, tools) {
  try {
    const did = await invoke("machine_setup", { name, tools });
    if (did.length) {
      machineHooks.set(name, [...did, "Sessions already open there pick them up after /hooks (or a restart)"]);
      toast(`${name}: ${did.join("; ")}`);
    } else if (!machineHooks.has(name)) machineHooks.set(name, ["Cue's hooks are set up there"]);
  } catch (e) { machineHooks.set(name, [`Couldn't set up Cue's hooks there: ${e}`]); }
  renderMain();
}
/** After Log in: check every 3 s (for 2 minutes) until the connection you opened is there. */
async function watchLogin(name) {
  for (let n = 0; n < 40; n++) {
    await new Promise((r) => setTimeout(r, 3000));
    if (!machinesList?.some((m) => m.name === name)) return;
    if ((await checkMachine(name))?.ok) return toast(`Connected to ${name}`);
  }
}
async function machAct(act, name) {
  try {
    if (act === "add") {
      const host = draft("mc-host").text.trim();
      if (!host) return toast("Type its SSH alias or user@host first");
      const m = await invoke("machine_add", { host, name: "" });
      draft("mc-host").text = "";
      machinesList = [...(machinesList || []), m];
      toast(`Added ${machShort(m.name)}`);
      // Added from + New: it's the machine this session starts on.
      if (nfAdding) { nfAdding = false; setNewMachine(m.name); draft("new-cwd").text = machineFolders(m.name)[0] || ""; }
      return checkMachine(m.name);
    }
    if (act === "remove") {
      await invoke("machine_remove", { name });
      machinesList = (machinesList || []).filter((m) => m.name !== name);
      machineState.delete(name);
      if (newMachine === name) setNewMachine("");
      toast(`Removed ${machShort(name)}`);
      return renderMain();
    }
    if (act === "check") return checkMachine(name);
    if (act === "rename") {
      machRenaming = machRenaming === name ? null : name;
      draft("mc-label").text = (machinesList || []).find((m) => m.name === name)?.label || "";
      renderMain();
      return machRenaming && document.querySelector('[data-text="mc-label"]')?.focus();
    }
    if (act === "rename-save") {
      const m = await invoke("machine_rename", { name, label: draft("mc-label").text });
      machinesList = (machinesList || []).map((x) => x.name === name ? m : x);
      machRenaming = null;
      toast(`Shown as ${machShort(name)} from now on`);
      return renderMain();
    }
    if (act === "login") {
      await invoke("machine_login", { name });
      toast(`Log in to ${name} in the terminal tab Cue opened. Cue never sees what you type there.`);
      return watchLogin(name);
    }
  } catch (e) { toast(`Couldn't: ${e}`); }
}
/** + New, under the Machine row when a machine is picked: how it stands, what's installed there (and how
 *  you'd install the rest yourself: Cue never installs anything on a machine), and Log in / Check / Remove. */
/** The machine whose Rename field is open in + New (null: none). */
let machRenaming = null;
function machinePanel(name) {
  const m = (machinesList || []).find((x) => x.name === name);
  if (!m) return "";
  const st = machineState.get(name);
  const checking = !st || st === "checking";
  const status = checking ? `<span class="mc-st">Checking…</span>`
    : st.ok ? `<span class="mc-st ok">Connected${st.os ? ` · ${esc(st.os)}` : ""}</span>`
    : st.needs_login ? `<span class="mc-st warn">Needs you to log in once</span>`
    : `<span class="mc-st bad">Can't reach it</span>`;
  const tools = !checking && st.ok ? `<div class="mc-tools">${Object.entries(TOOL_NAMES).map(([k, l]) => `<span class="mc-tool ${st.tools?.[k] ? "has" : ""}">${st.tools?.[k] ? "✓" : "–"} ${l}</span>`).join("")}</div>`
    : !checking && st.error ? `<div class="set-sub mono">${esc(st.error)}</div>` : "";
  const missing = !checking && st.ok ? Object.keys(TOOL_NAMES).filter((k) => !st.tools?.[k]) : [];
  const lines = [...(machineHooks.get(name) || []).map(esc), ...missing.map((k) => `${esc(TOOL_NAMES[k])} isn't installed there. To install it, run there: <code>${esc(st.help?.[k] || "")}</code>`)];
  const report = lines.length ? `<ul class="mc-report">${lines.map((l) => `<li>${l}</li>`).join("")}</ul>` : "";
  return `<div class="nf-row nf-note"><span></span><div class="nf-mpanel"><div class="nf-mhead"><span class="dim mono">${esc(m.host)}</span>${status}<span class="grow"></span>${!checking && st.needs_login ? `<button class="btn small primary" data-mach="login" data-name="${esc(name)}">Log in</button>` : ""}<button class="btn small" data-mach="rename" data-name="${esc(name)}">Rename</button><button class="btn small" data-mach="check" data-name="${esc(name)}">Check</button><button class="btn small" data-mach="remove" data-name="${esc(name)}">Remove</button></div>${machRenaming === name ? `<div class="mc-add"><input class="nf-in" data-text="mc-label" placeholder="${esc(m.name)}" spellcheck="false" autocomplete="off" value="${esc(draft("mc-label").text)}"/><button class="btn small primary" data-mach="rename-save" data-name="${esc(name)}">Save</button><button class="btn small" data-mach="rename" data-name="${esc(name)}">Cancel</button></div><span class="nf-hint wrap">What Cue shows for it everywhere. Empty: back to its own name.</span>` : ""}${tools}${report}</div></div>`;
}
/** Settings, in tabs so no one page runs long. The last tab you looked at opens next time. */
const SET_TABS = [["general", "General"], ["agents", "Agents"], ["chat", "Chat"], ["usage", "Usage"]];
let setTab = (() => { try { return localStorage.getItem("cue.setTab") || "general"; } catch { return "general"; } })();
function settingsSheet() {
  const st = state.settings || {};
  const c = state.connections || {};
  const keep = st.history?.keep ?? 300;
  const tab = SET_TABS.some(([k]) => k === setTab) ? setTab : "general";
  const pane = {
    general: `
    <div class="set-group">Updates</div>
    ${updateRow()}
    <div class="set-group">Look</div>
    ${setRow("Appearance", "Moss, light or dark. System follows macOS.", seg("appearance.mode", st.appearance?.mode ?? "system", [["system", "System"], ["light", "Light"], ["dark", "Dark"]]))}
    <div class="set-group">Notifications</div>
    ${setRow("Test", "Sends one macOS notification now. If nothing appears, check System Settings → Notifications → Cue.", `<button class="btn" data-act="test-notify">Send a test notification</button>`)}
    ${setRow("When an agent finishes its turn", "Tells you which session is waiting on you.", toggle("notify.finished", st.notify?.finished ?? true))}
    ${setRow("When an agent needs a decision", "Permissions and questions.", toggle("notify.decisions", st.notify?.decisions ?? true))}
    <div class="set-group">Mac</div>
    ${setRow("Open at login", "Start Cue when you log in to your Mac. It's listed under System Settings → General → Login Items.", toggle("login.open", st.login?.open ?? false))}
    ${setRow("Show icon in menu bar", "Off: no icon up top; open Cue from the Dock. Alerts still arrive as notifications.", toggle("tray.show", st.tray?.show ?? true))}`,
    agents: `
    <div class="set-group">Connected agents</div>
    ${agentRows()}
    ${extRows()}
    <div class="set-group">New sessions</div>
    ${st.sessions?.tmux_installed ? setRow("New sessions run in", `Cue: its terminal opens right in Cue, above the reply box, and keeps running if Cue quits (tmux runs it, out of sight). ${esc(st.sessions.terminal || "Terminal")}: a new tab there, as you'd open one. + New session can pick the other one for a session.`, seg("sessions.tmux", !!st.sessions?.tmux, [[true, "Cue"], [false, st.sessions.terminal || "Terminal"]])) : setRow("New sessions run in", `${esc(st.sessions?.terminal || "Terminal")}. To run them right in Cue instead, install tmux (<span class="mono">brew install tmux</span>) and reopen Settings.`, "")}
    <div class="set-group">Pi</div>
    ${setRow("Ask me before", "Pi doesn't ask on its own; Cue's extension adds the prompt.", seg("pi.gate", st.pi?.gate ?? "dangerous", [["dangerous", "Risky commands"], ["all", "Every command & edit"], ["off", "Never"]]))}
    <div class="set-group">Agents driving agents</div>
    ${setRow("Show sessions another agent drives in Waiting", "Off: when a helper agent finishes (one another agent started and labelled with CUE_DRIVEN_BY), its lead handles it, so it stays in Working as \"waiting on its lead\" with no card or notification. Turn on to treat them like your own sessions.", toggle("agents.show_driven", st.agents?.show_driven ?? false))}`,
    chat: `
    <div class="set-group">Quick phrases</div>
    ${setRow("Above the text box", `Click one to send it, after anything you've typed in the box. Up to 4. Empty a field to remove it.`, `<div class="quick-edit">${[0, 1, 2, 3].map((n) => `<input class="qp-in" data-quick-edit id="qp-${n}" value="${esc(qpEdit ? qpEdit[n] : phrases()[n] || "")}" maxlength="40" placeholder="${n < phrases().length ? "" : "Add one"}" aria-label="Quick phrase ${n + 1}"/>`).join("")}</div>`)}
    <div class="set-group">Chat shows</div>
    ${setRow("What the chat area shows", "Cue's chat: the conversation as Cue draws it from the transcript, with its steps. Its terminal (spike): the session's real terminal, full height, for sessions running in Cue (tmux); sessions in a terminal app still get Cue's chat.", seg("chat.mode", st.chat?.mode ?? "terminal", [["rendered", "Cue's chat"], ["terminal", "Its terminal"]]))}
    <div class="set-group">Steps</div>
    ${setRow("Steps in the chat", "What the agent did between your message and its reply, read from its transcript when you look. One line each: click a step for its output or diff.", seg("steps.mode", st.steps?.mode ?? "line", [["line", "One line each"], ["all", "Everything"], ["hidden", "Hidden"]]))}
    <div class="set-group">Context</div>
    ${setRow("Earlier in this session", "How many past exchanges (what it said or asked, what you answered) each card keeps.", seg("context.keep", st.context?.keep ?? 5, [[5, "5"], [10, "10"], [20, "20"]]))}
    ${setRow("When a Stop hook makes the agent continue, the card shows", "The answer: the agent's final reply, with the hook's output (e.g. a checklist) tucked underneath. The last message: whatever it wrote last, usually the hook's output.", seg("turn.mode", st.turn?.mode ?? "answer", [["answer", "The answer"], ["last", "The last message"]]))}
    <div class="set-group">History</div>
    ${setRow("Keep the last", `${state.history.length.toLocaleString()} saved now · ~/.cue/history.jsonl`, `<div style="display:flex;gap:8px;align-items:center">${seg("history.keep", keep, [[100, "100"], [300, "300"], [1000, "1,000"], [5000, "5,000"]])}<input class="num" type="number" min="10" step="50" value="${keep}" data-keep-custom aria-label="Custom history size"/></div>`)}`,
    usage: `
    <div class="set-group">Usage</div>
    ${setRow("Show Claude Code's limits", "The 5-hour and weekly meters from your Claude plan. Off: only your usage file's meters show.", toggle("usage.claude", st.usage?.claude ?? true))}
    ${setRow("Usage file", `Your own meters beside Claude Code's: a CSV you keep up to date (a proxy's budget, credits, tokens). Header <code>label,spent,limit,unit,resets_at</code>, then up to 3 rows; only <code>spent</code> is required. Cue only reads it, whenever it changes.`, `<input class="path-in" id="usage-file" data-usage-file value="${esc(st.usage?.file || "~/.cue/usage.csv")}" spellcheck="false" aria-label="Usage file"/>`)}`,
  }[tab];
  return `<div class="sheet"><div class="sheet-head"><h3>Settings</h3>${upd.current ? `<span class="set-ver">Cue ${esc(upd.current)}</span>` : ""}<span class="grow"></span><kbd>Esc</kbd></div><div class="set-tabs seg" role="tablist">${SET_TABS.map(([k, l]) => `<button role="tab" class="${tab === k ? "on" : ""}" aria-selected="${tab === k}" data-act="set-tab" data-tab="${k}">${l}</button>`).join("")}</div><div class="sheet-body">
    ${pane}
  </div></div>`;
}
