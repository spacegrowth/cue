// Cue UI — a Desk chat's two ways to read (#685, option C of #676): Conversation only folds each run of the Desk's own
// lines (what the lead gave, added, closed; Cue's messages; a session's CHECK…) into one row that opens in place;
// Everything shows every line, each with its small sign (desk/lead.js deskSign). The switch sits in the chat's head.

/** Each chat's way, by its session: "talk" (Conversation only); Everything when it isn't here. Kept on this Mac. */
const chatModes = (() => { try { return new Map(Object.entries(JSON.parse(localStorage.getItem("cue.chatModes") || "{}"))); } catch { return new Map(); } })();
/** Whether session `sid`'s chat shows the conversation only. */
const chatTalkOnly = (sid) => !!sid && chatModes.get(sid) === "talk";
function setChatMode(sid, mode) {
  if (!sid) return;
  if (mode === "talk") chatModes.set(sid, "talk"); else chatModes.delete(sid);
  try { localStorage.setItem("cue.chatModes", JSON.stringify(Object.fromEntries(chatModes))); } catch {}
  renderMain();
}
/** How many of the Desk's lines a chat's html has (each marked data-dk, a run of decisions one each). */
const dkCount = (html) => (String(html).match(/data-dk="(?!decided")/g) || []).length + (String(html).match(/<li data-dk-li>/g) || []).length;
/** The switch for session `sid`'s chat (as #665's): Conversation only | Everything, Everything with how many Desk
 *  lines the chat has (`html`, the chat drawn). */
function chatModeSwitch(sid, html = "") {
  if (!sid) return "";
  const talk = chatTalkOnly(sid), n = dkCount(html);
  const b = (mode, label, on, extra = "") => `<button class="seg2-b${on ? " on" : ""}" data-act="chat-mode" data-sid="${esc(sid)}" data-mode="${mode}" aria-pressed="${on}">${label}${extra}</button>`;
  return `<span class="seg2" role="group" aria-label="What the chat shows" data-chat-mode="${talk ? "talk" : "all"}">${b("talk", "Conversation only", talk)}${b("all", "Everything", !talk, n ? ` <em>${n}</em>` : "")}</span>`;
}
/** `html` with the Desk's mark on its first element: `kind` (route, cue, act…) and the item it's about. Kept as it is
 *  when it has one already. */
function dkMark(html, kind, id = "") {
  const s = String(html || "");
  if (!s || /^\s*<[a-z0-9]+\b[^>]*\bdata-dk="/i.test(s)) return s;
  return s.replace(/^(\s*)<([a-z0-9]+)/i, `$1<$2 data-dk="${esc(kind)}"${id ? ` data-dk-id="${esc(id)}"` : ""}`);
}
/** Whether a chat's row is wholly the Desk's (its first element marked; a reply of nothing but its lines, data-dkr). */
const dkRow = (html) => /^\s*<[a-z0-9]+\b[^>]*\bdata-dk(?:="|r\b)/i.test(String(html || ""));
/** The lead's moves (its START, CLOSE… lines, desk/lead.js DESK_ACTS, and Cue's own lines of them, desk/page.js
 *  deskActRows) as a run's summary says them: "started #12". */
const DK_MOVES = { start: "started", close: "closed", park: "parked", hold: "held", unpark: "unparked", drop: "dropped",
  "take-copy": "took back the copy of", copy: "took back the copy of", "land-first": "put first", "copy-first": "put first",
  reopen: "reopened", check: "how to check", send_back: "sent back", answer: "answered", loop: "drafted a loop for", title: "renamed", merge: "folded", see: "said what you'll see for" };
/** Runs open in place, by their key, until closed again. */
const dkOpenRuns = new Set();
/** What a run says in its folded row: "4 Desk updates · gave #15 and #12, a note on #9, #9 done, decided 1" (all of
 *  Cue's: "3 from Cue"). */
function dkRunSays(html) {
  const s = String(html), marks = [];
  const re = /data-dk="([^"]+)"(?: data-dk-id="([^"]*)")?/g;
  let m;
  while ((m = re.exec(s))) marks.push({ kind: m[1], id: m[2] || "", at: m.index });
  // The item each is about: its own mark's, else the first item link after it, before the next mark.
  marks.forEach((x, n) => {
    if (x.id) return;
    const part = s.slice(x.at, marks[n + 1]?.at ?? s.length);
    x.id = /data-desk-chk-item="([^"]+)"/.exec(part)?.[1] || "";
  });
  const decided = (s.match(/<li data-dk-li>/g) || []).length;
  const n = marks.filter((x) => x.kind !== "decided").length + decided;
  const ids = (k) => [...new Set(marks.filter((x) => x.kind === k && x.id).map((x) => `#${x.id}`))];
  const and = (xs) => (xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs.at(-1)}`);
  const of = (k) => marks.filter((x) => x.kind === k).length;
  const parts = [];
  const gave = ids("route"); if (gave.length) parts.push(`gave ${and(gave)}`);
  const added = ids("item"); if (added.length) parts.push(`added ${and(added)}`); else if (of("item")) parts.push(`added ${of("item")}`);
  const notes = ids("note"); if (notes.length) parts.push(notes.length > 1 ? `notes on ${and(notes)}` : `a note on ${notes[0]}`);
  const done = ids("done"); if (done.length) parts.push(`${and(done)} done`);
  for (const [k, v] of Object.entries(DK_MOVES)) { const xs = ids(k); if (xs.length) parts.push(`${v} ${and(xs)}`); }
  const known = new Set(["route", "item", "note", "done", "decided", "minor", "ready", "cue", ...Object.keys(DK_MOVES)]);
  const rest = marks.filter((x) => !known.has(x.kind) || (DK_MOVES[x.kind] && !x.id)).length;
  if (decided) parts.push(`decided ${decided}`);
  if (of("minor")) parts.push(of("minor") > 1 ? `${of("minor")} small calls` : "a small call");
  if (of("ready")) parts.push(of("ready") > 1 ? `${of("ready")} to check` : "one to check");
  const cue = of("cue");
  const label = n && cue === n ? `${n} from Cue` : `${n || 1} Desk update${n === 1 ? "" : "s"}`;
  if (cue && cue !== n) parts.push(`${cue} from Cue`);
  if (rest) parts.push(`${rest} more`);
  const said = parts.slice(0, 4).join(", ") + (parts.length > 4 ? ", …" : "");
  return { label, said };
}
/** A run of the Desk's lines folded to one row (`key` keeps it open across draws); a click opens it in place, the
 *  lines under it as Everything draws them. */
function dkRunHtml(key, html) {
  const open = dkOpenRuns.has(key), { label, said } = dkRunSays(html);
  return `<div class="dk-run${open ? " open" : ""}" data-dk-run="${esc(key)}"><button class="dk-run-b" data-act="desk-run" data-key="${esc(key)}" aria-expanded="${open}">`
    + `<span class="dk-run-pm" aria-hidden="true">${open ? "−" : "+"}</span><b>${esc(label)}</b>${said ? `<span class="dk-run-s"> · ${esc(said)}</span>` : ""}</button>`
    + `${open ? `<div class="dk-run-in">${html}</div>` : ""}</div>`;
}
/** A chat's rows (chatHtml's) with each run of the Desk's own rows folded to one (dkRunHtml), keyed by its first. Rows
 *  drawn as nothing don't break a run. */
function dkFoldRows(rows) {
  const out = [];
  let run = [];
  const flush = () => {
    if (!run.length) return;
    const key = `run:${/data-key="([^"]+)"/.exec(run[0])?.[1] || out.length}`;
    out.push(keyed(dkRunHtml(key, run.join("")), key));
    run = [];
  };
  for (const r of rows) {
    if (!r) continue;
    if (dkRow(r)) run.push(r); else { flush(); out.push(r); }
  }
  flush();
  return out;
}
/** An agent's reply in Conversation only: md's blocks of the Desk's lines (own-lines, decided) taken out as they're
 *  drawn (md `run`), each run of them folded in place (`key`: the reply's). `{ html, all }`: `all` when the reply is
 *  nothing but such lines (the row then joins the runs around it). */
function dkReplyRuns(draw, key) {
  const runs = [];
  const html = draw((h) => { runs.push(h); return `\u0001${runs.length - 1}\u0001`; });
  if (!runs.length) return { html, all: false };
  const all = !html.replace(/\u0001\d+\u0001/g, "").replace(/<[^>]*>/g, "").trim();
  if (all) return { html: html.replace(/\u0001(\d+)\u0001/g, (_, n) => runs[n]), all: true };
  let k = 0;
  const folded = html.replace(/(?:\u0001\d+\u0001\s*)+/g, (seq) => dkRunHtml(`${key}:r${k++}`, [...seq.matchAll(/\u0001(\d+)\u0001/g)].map((m) => runs[m[1]]).join("")));
  return { html: folded, all: false };
}
/** A run's row clicked (`el`, its button): opened or folded, the row staying where it was on screen (the chat would
 *  otherwise keep its end in place and move the row up as the lines open under it). */
function dkRunFlip(el) {
  const k = el.dataset.key, y = el.getBoundingClientRect().top;
  if (dkOpenRuns.has(k)) dkOpenRuns.delete(k); else dkOpenRuns.add(k);
  // The Desk's chat keeps its end in view after a draw (desk/view.js deskSettle): not this time, the row stays.
  if (el.closest(".dk-chat") && typeof deskChatEnd !== "undefined") deskChatEnd = false;
  renderMain();
  const now = [...document.querySelectorAll("[data-dk-run]")].find((x) => x.dataset.dkRun === k)?.querySelector(".dk-run-b");
  let box = now?.parentElement;
  while (box && box !== document.body && !(box.scrollHeight > box.clientHeight && /auto|scroll/.test(getComputedStyle(box).overflowY))) box = box.parentElement;
  if (now && box && box !== document.body) box.scrollTop += now.getBoundingClientRect().top - y;
}
