// Cue UI — the Desk's own lines out of its chats (#688, option D of the chat audit): what the lead gave, added, closed,
// noted; Cue's messages to the lead and its sessions; hand-offs, small calls… Each run of them between your turns is one
// plain count line in the chat ("4 Desk updates · gave #15, #9 done · Activity ›"), a click opening the Activity tab,
// where every one of them is. Lines meant for you stay in the chat as they are (DK_FOR_YOU).

/** The Desk's lines that stay in a chat: How to check, If you pick, For you to try, Not done, Ready to check, Stopped;
 *  the band between Desks (TELL DESK, TELL DESKS), your picks ("You picked B2 on #d5") and your words Cue passed on
 *  (chat/render.js) aren't the Desk's own either; nor a loop the lead drafted for you to start (its card's Start loop). */
const DK_FOR_YOU = new Set(["check", "if-picked", "user-verify", "not-done", "ready", "stopped", "tell-desk", "tell-desks", "picked", "yours", "loop"]);
/** `html` with the Desk's mark on its first element: `kind` (route, cue, act…) and the item it's about. Kept as it is
 *  when it has one already. */
function dkMark(html, kind, id = "") {
  const s = String(html || "");
  if (!s || /^\s*<[a-z0-9]+\b[^>]*\bdata-dk="/i.test(s)) return s;
  return s.replace(/^(\s*)<([a-z0-9]+)/i, `$1<$2 data-dk="${esc(kind)}"${id ? ` data-dk-id="${esc(id)}"` : ""}`);
}
/** Whether a chat's row is wholly the Desk's and leaves the chat: its first element marked with a kind not meant for
 *  you, or a reply of nothing but such lines (data-dkr). */
function dkRow(html) {
  const m = /^\s*<[a-z0-9]+\b[^>]*?\bdata-dk(?:="([^"]*)"|r\b)/i.exec(String(html || ""));
  return !!m && !DK_FOR_YOU.has(m[1]);
}
/** A row of yours (your bubble), which ends a run. */
const dkYours = (html) => /^\s*<[a-z0-9]+\b[^>]*\bclass="[^"]*\bmg-you\b/i.test(String(html || ""));
/** The lead's moves (its START, CLOSE… lines, desk/lead.js DESK_ACTS, and Cue's own lines of them, desk/page.js
 *  deskActRows) as a count line's gist says them: "started #12". */
const DK_MOVES = { start: "started", close: "closed", park: "parked", hold: "held", unpark: "unparked", drop: "dropped",
  "take-copy": "took back the copy of", copy: "took back the copy of", "land-first": "put first", "copy-first": "put first",
  reopen: "reopened", send_back: "sent back", answer: "answered", loop: "drafted a loop for", title: "renamed", merge: "folded", see: "said what you'll see for" };
/** What a run of the Desk's lines (`html`) says: { n, label: "4 Desk updates", said: "gave #15 and #12, #9 done" }. */
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
  const known = new Set(["route", "item", "note", "done", "decided", "minor", "cue", "to-lead", ...Object.keys(DK_MOVES)]);
  const rest = marks.filter((x) => !known.has(x.kind) || (DK_MOVES[x.kind] && !x.id)).length;
  if (decided) parts.push(`decided ${decided}`);
  if (of("minor")) parts.push(of("minor") > 1 ? `${of("minor")} small calls` : "a small call");
  if (of("to-lead")) parts.push(of("to-lead") > 1 ? `${of("to-lead")} to the lead` : "one to the lead");
  const cue = of("cue");
  if (cue) parts.push(`${cue} from Cue`);
  if (rest) parts.push(`${rest} more`);
  const said = parts.slice(0, 3).join(", ") + (parts.length > 3 ? ", …" : "");
  const c = n || 1;
  return { n: c, label: `${c} Desk update${c === 1 ? "" : "s"}`, said };
}
/** A run's count line: how many, what they were, and Activity ›; a click opens the Activity tab. The newest has a dot. */
function dkCountHtml(key, html, newest) {
  const { label, said } = dkRunSays(html);
  return `<div class="dk-cnt${newest ? " new" : ""}" data-dk-cnt="${esc(key)}"><button class="dk-cnt-b" data-act="desk-activity">`
    + `${newest ? '<i class="dk-cnt-dot" aria-hidden="true"></i>' : ""}<b>${esc(label)}</b>${said ? `<span class="dk-cnt-s"> · ${esc(said)}</span>` : ""}`
    + `<span class="dk-cnt-a"> · Activity ›</span></button></div>`;
}
/** The Desk's lines a reply's body gave up (dkReplyTake), kept in its row for dkFoldRows. */
const DK_TAKEN = /<template class="dk-taken">([\s\S]*?)<\/template>/g;
/** A Desk chat's rows (chatHtml's): each run of the Desk's own rows between your turns taken out, and one count line
 *  (dkCountHtml) where the run began. A reply's own Desk lines (dkReplyTake) count in the run around it. Drawing a chat
 *  changes nothing else: Activity works its rows out from the session's messages (dkActivityOf). */
function dkFoldRows(rows) {
  const out = [];
  let run = null;
  const flush = () => {
    if (run?.html.length) out[run.slot] = keyed(dkCountHtml(run.key, run.html.join(""), false), run.key);
    else if (run) out[run.slot] = "";
    run = null;
  };
  const add = (html, key) => {
    if (!run) { run = { slot: out.length, key: `cnt:${key}`, html: [] }; out.push(""); }
    run.html.push(html);
  };
  for (const r0 of rows) {
    if (!r0) continue;
    const key = /data-key="([^"]+)"/.exec(r0)?.[1] || String(out.length);
    // A reply's own Desk lines (dkReplyTake) count one by one; a reply of nothing else leaves with them.
    let r = r0, k = 0;
    const lines = r0.includes('<template class="dk-taken">');
    if (dkRow(r0) && !lines) { add(r0, key); continue; }
    if (dkYours(r0)) flush();
    // Its lines counted after it, where they came.
    const own = [];
    if (lines) r = r.replace(DK_TAKEN, (_, h) => { own.push(h); return ""; });
    if (!dkRow(r0)) out.push(r);
    for (const h of own) add(h, `${key}:${k++}`);
  }
  // The newest run: its line has the dot, and grows as lines come in.
  if (run?.html.length) out[run.slot] = keyed(dkCountHtml(run.key, run.html.join(""), true), run.key);
  else if (run) out[run.slot] = "";
  return out.filter(Boolean);
}
/** The kind a line's mark gives it (dkMark's), "" for an unmarked one. */
const dkKindOf = (html) => /^\s*<[a-z0-9]+\b[^>]*?\bdata-dk="([^"]*)"/i.exec(String(html || ""))?.[1] || "";
/** A block md drew with `lines` (`h`; an own-lines block brings its lines, `own`): what stays in the reply, and the Desk's
 *  lines taken out of it, each its own Activity row. Lines meant for you (or unmarked) stay, in place. */
function dkTake(h, own) {
  if (!own) return { keep: "", taken: [h] };
  const kept = [], taken = [];
  for (const l of own) {
    const k = dkKindOf(l);
    if (!k || DK_FOR_YOU.has(k)) kept.push(l);
    else taken.push(`<div class="own-lines">${l}</div>`);
  }
  return { keep: kept.length ? `<div class="own-lines">${kept.join("")}</div>` : "", taken };
}
/** An agent's reply with md's blocks of the Desk's lines (own-lines, decided) taken out as they're drawn (md `run`,
 *  dkTake): those meant for you stay. Each taken line rides along in its own <template> (dkFoldRows reads them); `all`
 *  when the reply was nothing but such lines (its row then leaves whole). */
function dkReplyTake(draw) {
  const taken = [];
  const html = draw((h, own) => {
    const t = dkTake(h, own);
    taken.push(...t.taken);
    return t.keep;
  });
  if (!taken.length) return { html, all: false };
  const all = !html.replace(/<[^>]*>/g, "").trim() && !/<(?:img|svg|button|pre|table)\b/i.test(html);
  const tpl = taken.map((x) => `<template class="dk-taken">${x}</template>`).join("");
  return all ? { html: tpl, all: true } : { html: html + tpl, all: false };
}
/** Session `s`'s Desk lines, drawn now (`page`: its Desk chat's way, the lead's with its short lines, deskLeadNote):
 *  Cue's card or message for your-side message `e`, or each Desk line taken out of reply `e`, in order. */
function dkLinesOf(s, e, n, page) {
  const sid = s.session_id;
  if (e.role === "you") {
    const h = !(typeof deskToldBand === "function" && deskToldBand(sid, e)) && !youBack(e) && chatCueRow(page, s, sid, e, n, new Map());
    return h && dkRow(h) ? [h] : [];
  }
  if (e.role !== "agent") return [];
  const out = [];
  md(addOn.reply(sid, e.text).text, { runIn: sid, lines: deskLineHtml, run: (h, own) => { const t = dkTake(h, own); out.push(...t.taken); return t.keep; } });
  return out;
}
/** Which of session `s`'s messages give Activity rows, worked out from its messages, never from a drawing of its chat
 *  (a page folded to one item's turns can't change them): [{ at: its message's time, key, e, n, k: its line in it,
 *  calls: a DECIDED block's words }], oldest first. Each row's html is drawn as it's read, so it's never stale. */
function dkActivityRows(s) {
  const ex = conversation(null, s).map(deskSaidE), out = [];
  const { asks, answers, decided } = chatAsks(ex);
  const page = dkActivityPage(s.session_id);
  ex.forEach((e, n) => {
    if (answers.has(n) || decided.has(n) || asks.has(n)) return;
    dkLinesOf(s, e, n, page).forEach((h, k) => out.push({ at: e.at_ms, key: `${e.role[0]}:${e.at_ms}:${k}`, e, n, k,
      calls: [...h.matchAll(/<span class="dec-what">([\s\S]*?)<\/span>/g)].map((m) => m[1].replace(/<[^>]*>/g, "")) }));
  });
  return out;
}
/** How session `sid`'s Desk chat draws Cue's messages: the lead's with its short lines (deskChat, deskSessChat). */
const dkActivityPage = (sid) => ({ lines: deskLineHtml, you: (state.desk?.projects || []).some((x) => deskLead(x)?.sid === sid) ? deskLeadNote : deskCueNote });
/** dkActivityRows by session, kept till its messages change: { sig, rows }. */
const dkActivity = new Map();
/** A message's lines as last drawn this turn (dkLinesOf), so its rows draw it once. */
const dkDrawn = new Map();
/** Activity for session `sid`, newest first, one reply's lines in their own order: [{ at, key, sid, calls, html }]. */
function dkActivityOf(sid) {
  const s = typeof sessionOf === "function" && sessionOf(sid);
  if (!s) return [];
  const msgs = conversation(null, s), last = msgs.at(-1), page = dkActivityPage(sid);
  const sig = `${msgs.length}:${last?.at_ms ?? 0}:${String(last?.text ?? "").length}:${page.you === deskLeadNote}`;
  let hit = dkActivity.get(sid);
  if (!hit || hit.sig !== sig) {
    const rows = dkActivityRows(s).map((x) => ({ ...x, sid, get html() {
      if (!dkDrawn.size) queueMicrotask(() => dkDrawn.clear());
      const id = `${sid}:${x.n}:${x.e.at_ms}`;
      if (!dkDrawn.has(id)) dkDrawn.set(id, dkLinesOf(s, x.e, x.n, dkActivityPage(sid)));
      return dkDrawn.get(id)[x.k] || "";
    } })).sort((x, y) => y.at - x.at);
    hit = { sig, rows };
    dkActivity.set(sid, hit);
  }
  return hit.rows;
}
/** A row taken from a chat (dkActivity's) as one line for a panel: its head's words (a line's .mg-lh, a card's who, tag
 *  and item), its item links kept. */
function dkRowLine(html) {
  if (typeof document === "undefined") return esc(String(html).replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").trim());
  const t = document.createElement("template");
  t.innerHTML = html;
  const el = t.content.firstElementChild;
  if (!el) return "";
  el.querySelectorAll(".mg-age, .mg-mk, .dk-sg, .mg-f, svg:not(.dk-ki svg)").forEach((x) => x.remove());
  const lh = el.querySelector(".mg-lh");
  if (lh) return lh.innerHTML;
  const h = el.querySelector(".mg-h");
  const body = (el.querySelector(".mg-t, .mg-line, .mg-full, .dec-what, .mg-text") || el).textContent.replace(/\s+/g, " ").trim();
  const cut = body.length > 90 ? `${body.slice(0, 89)}…` : body;
  return `${h ? `${h.innerHTML} · ` : ""}${esc(cut)}`;
}
/** Opens the open Desk's Activity tab (a count line's click, the panel's "N more in Activity ›"). */
function deskOpenActivity() {
  // Whatever is open (an item, a session's chat) takes the lead's card's place (deskOpenCard): close it to show the tab.
  deskOpen = null;
  deskTab = "moves";
  renderMain();
}
