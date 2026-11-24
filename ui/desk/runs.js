// Cue UI — the Desk's own lines out of its chats (#688; #789: gone from the chats altogether): what the lead gave,
// added, closed, noted; Cue's messages to the lead and its sessions (hand-offs, sent back, landed, stopped); Ready to
// check… None of them shows in a chat: the chat is only the conversation, and every one of them is in the Desk's
// Activity tab (desk/page.js deskActivity), and an item's on its own page. Lines meant for you stay (DK_FOR_YOU).

/** The Desk's lines that stay in a chat: How to check, If you pick, For you to try, Not done;
 *  the band between Desks (TELL DESK, TELL DESKS), your picks ("You picked B2 on #d5") and your words Cue passed on
 *  (chat/render.js) aren't the Desk's own either; nor a loop the lead drafted for you to start (its card's Start loop). */
const DK_FOR_YOU = new Set(["check", "if-picked", "user-verify", "not-done", "tell-desk", "tell-desks", "picked", "yours", "loop"]);
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
/** The Desk's lines a reply's body gave up (dkReplyTake), kept in its row for dkFoldRows. */
const DK_TAKEN = /<template class="dk-taken">([\s\S]*?)<\/template>/g;
/** A Desk chat's rows (chatHtml's) with the Desk's own rows taken out (#789: no count line in their place either), and
 *  a reply's own Desk lines (dkReplyTake) dropped from it; a reply of nothing but those leaves whole. Drawing a chat
 *  changes nothing else: Activity works its rows out from the session's messages (dkActivityOf). */
const dkFoldRows = (rows) => rows.filter((r) => r && !dkRow(r)).map((r) => (r.includes('<template class="dk-taken">') ? r.replace(DK_TAKEN, "") : r));
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
/** Opens the open Desk's Activity tab (the panel's "N more in Activity ›"). */
function deskOpenActivity() {
  // Whatever is open (an item, a session's chat) takes the lead's card's place (deskOpenCard): close it to show the tab.
  deskOpen = null;
  deskTab = "moves";
  renderMain();
}
