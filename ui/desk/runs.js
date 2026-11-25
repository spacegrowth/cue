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
  let forYou = false;
  const html = draw((h, own) => {
    const t = dkTake(h, own);
    taken.push(...t.taken);
    if (t.keep && /\bdata-dk="/.test(t.keep)) forYou = true;
    return t.keep;
  });
  const acts = taken.some(dkActs);
  // Nothing left to read once its Desk lines are out (#837): it leaves whole, never an empty reply in the chat.
  const all = !html.replace(/<[^>]*>/g, "").trim() && !/<(?:img|svg|button|pre|table)\b/i.test(html);
  if (!taken.length) return { html, all, acts, forYou };
  const tpl = taken.map((x) => `<template class="dk-taken">${x}</template>`).join("");
  return all ? { html: tpl, all: true, acts, forYou } : { html: html + tpl, all: false, acts, forYou };
}
/** Whether a Desk line taken out of a reply acts on something (#816: ROUTE, ITEM, DONE, START, NOTE…), not only a call
 *  it made (DECIDED). */
const dkActs = (h) => { const k = dkKindOf(h) || dkKindOf(String(h).replace(/^\s*<div class="own-lines">/, "")); return !!k && k !== "decided" && k !== "minor"; };
/** Whether your-side message `e` (the `n`th) of session `s`'s chat is wholly Cue's notice, out of the chat for Activity
 *  (#816): not your words, not your words that went with a session's (#812), not your answer to a question. */
function dkNotice(s, e, n, page) {
  if (e.role !== "you" || e.decision) return false;
  if (chatFrom(e) !== "cue" && typeof deskRelaySplit === "function" && deskRelaySplit(e.text)?.yours) return false;
  return dkLinesOf(s, e, n, page).length > 0;
}
/** Reply `n` of `ex` (session `s`'s messages, their Desk summaries dropped) when the turn it answers was only Cue's
 *  notices (#816): { turn: those notices' indexes, oldest first; stays: it stays in the chat (it holds Desk lines that
 *  act on something, or lines for you, or a card), with the notices one quiet line over it, else it goes to Activity
 *  under its last notice }. null for any other reply: a turn with your words in it stays as it is. */
function dkNoticeReply(s, ex, n, page) {
  const e = ex[n];
  if (e?.role !== "agent") return null;
  // In a lead's chat, a turn with any message marked Cue's is decided by the tags (chat/who.js chatTurnOf, #837): one
  // Cue started is Activity's whole, every reply in it under its last notice, whatever the reply does (its Desk lines
  // are Activity's rows of their own; lines for you, pictures and buttons drawn there in full, dkNoticeAnswer). Cue's
  // replies to the lead's own lines ("answer") start no turn: a reply after one goes on with the turn before it.
  if (page?.you === deskLeadNote) {
    const turn = chatTurnOf(ex, n);
    // From the reply before its turn up to it: any of Cue's there (its turn's, or Cue's answers among the replies).
    let j = n - 1;
    while (j >= 0 && (chatFrom(ex[j]) === "ai" || chatCueAnswer(ex[j]))) j--;
    while (j >= 0 && chatFrom(ex[j]) !== "ai") j--;
    const tagged = ex.slice(j + 1, n).some((x) => chatFrom(x) === "cue");
    if (tagged) return turn.length && turn.every((k) => chatFrom(ex[k]) === "cue") ? { turn, stays: false, tagged: true } : null;
  }
  let i = n - 1;
  while (i >= 0 && ex[i].role === "agent") i--;
  const turn = [];
  for (; i >= 0 && ex[i].role !== "agent"; i--) {
    if (!dkNotice(s, ex[i], i, page)) return null;
    turn.unshift(i);
  }
  if (!turn.length) return null;
  // An answer to a session's question (#831): Activity's with the question, whatever it does; your chat holds only
  // your words and the lead's answers to them.
  if (turn.some((i) => typeof deskRelaySplit === "function" && deskRelaySplit(ex[i].text))) return { turn, stays: false };
  const sid = s.session_id, said = addOn.reply(sid, e.text);
  const t = dkReplyTake((run) => md(said.text, { runIn: sid, lines: page?.lines || deskLineHtml, run }));
  return { turn, stays: !!(t.acts || t.forYou || said.after || e.images?.length) };
}
/** The notices of a turn (`turn`: their indexes in `ex`) as one quiet line over the lead's reply that acted on them
 *  (#816): "Cue · Stuck #815 · Queued #812". */
function dkNoticeQuiet(s, ex, turn, page) {
  const one = (i) => {
    const c = cueMsgOf(ex[i].text);
    return c ? `${esc(c.tag)}${c.id ? ` #${esc(c.id)}` : ""}${c.what ? `: ${esc(c.what)}` : ""}` : dkLinesOf(s, ex[i], i, page).map((h) => dkRowLine(h)).join(" · ");
  };
  const said = turn.map(one).filter(Boolean);
  if (!said.length) return "";
  return `<div class="dk-quiet">${cueMark()}<b class="mg-who">Cue</b><span class="dk-quiet-w">${said.join(" · ")}</span></div>`;
}
/** The lead's reply `e` to a notice-only turn, under its last notice in Activity (#816): its name, then its words, folded. */
function dkNoticeAnswer(s, e) {
  const text = addOn.plain(e.text), d = (state.desk?.projects || []).find((x) => deskLead(x)?.sid === s.session_id);
  const who = d ? deskLeadName(d) : nameOf(s.session_id, s.project);
  // One with more than words (#837: lines for you with their buttons, pictures, an add-on's part): drawn in full as the
  // chat would, its Desk lines left out (they're Activity's rows of their own).
  const said = addOn.reply(s.session_id, e.text);
  const t = dkReplyTake((run) => md(said.text, { runIn: s.session_id, lines: deskLineHtml, refs: deskRefs, run }));
  if (t.forYou || said.after || e.images?.length) {
    const body = t.html.replace(DK_TAKEN, "");
    return `<div class="dk-ans full" data-dk-ans="${e.at_ms}"><b class="mg-who">${esc(who)}</b><div class="msg mg-text cv-text">${body}</div>${said.after}${thumbs(e.images)}</div>`;
  }
  return `<div class="dk-ans" data-dk-ans="${e.at_ms}"><b class="mg-who">${esc(who)}</b>${deskCut(`ans:${s.session_id}:${e.at_ms}`, "mg-full", esc(text).replace(/\n/g, "<br>"), text)}</div>`;
}
/** Session `s`'s Desk lines, drawn now (`page`: its Desk chat's way, the lead's with its short lines, deskLeadNote):
 *  Cue's card or message for your-side message `e`, or each Desk line taken out of reply `e`, in order. */
function dkLinesOf(s, e, n, page, answer = null) {
  const sid = s.session_id;
  // The lead's answers to it (`answer`: one, or each reply of its turn), when they left the chat: under it, in order.
  const said = [].concat(answer || []).map((a) => dkNoticeAnswer(s, a)).join("");
  const withAnswer = (h) => (said ? h.replace(/<\/div>\s*$/, `${said}</div>`) : h);
  // Another Desk's band, marked Cue's, in a lead's chat (#837): Activity's, the lead's answer under it.
  if (chatFrom(e) === "cue" && chatPlace(e, page?.you === deskLeadNote ? "lead" : "session") === "activity") {
    const band = e.role === "peer" ? (typeof deskPeerBand === "function" && deskPeerBand(sid, e)) || peerBubble(e.from || "another agent", "", e.at_ms, e.text)
      : typeof deskToldBand === "function" && deskToldBand(sid, e);
    // The answers under the band, not inside it (its tint is that Desk's words only).
    if (band) return [`<div data-dk="peer" class="dk-peer-row">${band}${said}</div>`];
  }
  if (e.role === "you") {
    // Marked Cue's (#837, chat/who.js): Cue's whatever its words say; unmarked, a send-back of yours stays yours.
    const h = !(typeof deskToldBand === "function" && deskToldBand(sid, e)) && (chatFrom(e) === "cue" || !youBack(e)) && chatCueRow(page, s, sid, e, n, new Map());
    if (!h || !dkRow(h)) return [];
    // The lead's answer to it, when that answer left the chat (#816): under it, in the same row.
    return [withAnswer(h)];
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
  // A lead's answers that left its chat (#816), by the notice they go under: its turn's last; every reply of that turn,
  // in order (#837).
  const lead = page.you === deskLeadNote, answered = new Map();
  if (lead) ex.forEach((e, n) => { const r = e.role === "agent" && !asks.has(n) && dkNoticeReply(s, ex, n, page); if (r && !r.stays) answered.set(r.turn.at(-1), [...(answered.get(r.turn.at(-1)) || []), e]); });
  ex.forEach((e, n) => {
    if (answers.has(n) || decided.has(n) || asks.has(n)) return;
    const answer = answered.get(n) || null;
    dkLinesOf(s, e, n, page, answer).forEach((h, k) => out.push({ at: e.at_ms, key: `${e.role[0]}:${e.at_ms}:${k}`, e, n, k, answer,
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
      if (!dkDrawn.has(id)) dkDrawn.set(id, dkLinesOf(s, x.e, x.n, dkActivityPage(sid), x.answer));
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
