// Cue UI box — the message box's markup: the words, the mic, a picture, Send. One builder for every box that sends
// (a session's chat box, the Desk's boxes); each says how it looks through its options. Nothing calls it yet: the chat
// box (chat/compose.js box) and the Desk's (desk_view.js deskBox, deskItemFoot, desk_act.js deskBackHtml, desk_page.js
// deskGoBackHtml) move onto it one at a time (docs/refactor-plan.md, Phase 3).

/** A message box's HTML. `o`:
 *  - key: its draft key (data-text, data-mic, data-attach, data-drop). text: its words (the draft's when left out).
 *  - look: "desk" (.dk-box, the default) or "composer" (a session's chat box: .composer > .field).
 *  - placeholder, label (aria-label when no suggestion), rows (1 for "desk", 2 for "composer").
 *  - sug: Claude's suggested reply laid over the empty box ("" for none); sugSid: the session it's from (the Desk's
 *    data-sug-sid; left out for none). wrap: the textarea in a .ta-wrap with the suggestion's line (always for "composer").
 *  - top: what goes above the pictures (quick phrases, the "To …" line). chips: the box's pictures (true by default).
 *  - drop: pictures can be dropped on it (data-drop). mic, attach: its buttons (both by default; "composer" without
 *    `drop` has no attach button).
 *  - send: { attrs, label, disabled } for its Send. extra: buttons before Send (a "desk" box's, and "composer"'s
 *    always-shown `extra` with its `alt` already added when something's typed).
 *  - head: { label, cancel } puts a "desk" box under a title and a Cancel (.dk-sb, Send back, Go back); cancel is the
 *    Cancel button's attributes. sbClass: more classes for that .dk-sb.
 *  - "composer" only: bang (a "!" line typed), cmd (the "/" menu's slot), clock (the "Send at" button), below (under
 *    the row: the "Send at" bar and the "/" hint).
 *  No hover text: say what a button does in its aria-label. */
function boxHtml(o) {
  const key = o.key, k = esc(key);
  const text = o.text ?? draft(key).text;
  const on = dictating?.key === key;
  const chips = o.chips === false ? "" : draftChips(key);
  const sug = o.sug || "";
  const send = o.send || {};
  if (o.look === "composer") {
    const ta = `<textarea rows="${o.rows || 2}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off" data-text="${k}"${sug ? ` class="sug" aria-label="${esc(`Suggested: ${sug}. Tab types it in.`)}"` : ""} placeholder="${esc(sug ? " " : o.placeholder || "")}">${esc(text)}</textarea>`;
    const mic = o.mic === false ? "" : `<button class="icon-btn mic ${on ? "on" : ""}" data-mic="${k}" aria-label="Dictate">${MIC_ICON}</button>`;
    const attach = o.drop && o.attach !== false ? `<button class="icon-btn" data-attach="${k}" aria-label="Add image">${IMAGE_ICON}</button>` : "";
    return `<div class="composer"><div class="field ${o.drop ? "drop" : ""} ${o.bang ? "bang" : ""}" data-drop="${o.drop ? k : ""}">${o.cmd || ""}${o.top || ""}${o.drop ? chips : ""}<div class="field-row">
    <div class="ta-wrap">${ta}${sugLineHtml(sug)}</div>
    ${mic}
    ${attach}
    ${o.clock || ""}
    ${o.extra || ""}<button class="btn primary send" ${send.attrs || ""} ${send.disabled ? "disabled" : ""}>${send.label ?? "Send"}</button></div>${o.below || ""}</div></div>`;
  }
  const label = sug ? ` aria-label="${esc(`Suggested: ${sug}. Tab types it in.`)}"` : o.label ? ` aria-label="${esc(o.label)}"` : "";
  const ta = `<textarea rows="${o.rows || 1}" data-text="${k}"${sug ? ` class="sug"${o.sugSid != null ? ` data-sug-sid="${esc(o.sugSid)}"` : ""}` : ""}${label} placeholder="${esc(sug ? " " : o.placeholder || "")}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(text)}</textarea>`;
  const field = o.wrap ? `<div class="ta-wrap">${ta}${sugLineHtml(sug)}</div>` : ta;
  const mic = o.mic === false ? "" : `<button class="icon-btn mic ${on ? "on" : ""}" data-mic="${k}" aria-label="${on ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button>`;
  const attach = o.attach === false ? "" : `<button class="icon-btn" data-attach="${k}" aria-label="Add an image">${IMAGE_ICON}</button>`;
  const go = `<button class="btn primary small" ${send.attrs || ""}${send.disabled ? " disabled" : ""}>${send.label ?? "Send"}</button>`;
  const inner = `<div class="dk-box${o.head ? " dk-sb-box" : ""}"${o.drop === false ? "" : ` data-drop="${k}"`}>${o.top || ""}${chips}<div class="dk-box-row">${field}${mic}${attach}${o.extra || ""}${go}</div></div>`;
  if (!o.head) return inner;
  return `<div class="dk-sb${o.sbClass ? ` ${o.sbClass}` : ""}"><div class="dk-sb-h"><span class="dk-sb-l">${esc(o.head.label)}</span><button class="dk-sb-x" ${o.head.cancel || ""}>Cancel</button></div>${inner}</div>`;
}
