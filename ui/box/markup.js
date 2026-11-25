// Cue UI box — the message box's markup: the words, the mic, a picture, Send. One builder for every box that sends
// (a session's chat box, the Desk's boxes, the mini panel's, btw's, a forward's note); each says how it
// looks through its options (docs/refactor-plan.md, Phase 3).

/** A message box's HTML. `o`:
 *  - key: its draft key (data-text, data-mic, data-attach, data-drop). text: its words (the draft's when left out).
 *  - look: "desk" (.dk-box, the default) or "composer" (a session's chat box: .composer > .field).
 *  - placeholder, label (aria-label when no suggestion), rows (1 for "desk", 2 for "composer").
 *  - sug: Claude's suggested reply laid over the empty box ("" for none); sugSid: the session it's from (the Desk's
 *    data-sug-sid, and a "composer" box's whose key isn't its session's "s:…"; left out for none). wrap: the textarea in a .ta-wrap with the suggestion's line (always for "composer").
 *  - top: what goes above the pictures (quick phrases, the "To …" line). chips: the box's pictures (true by default).
 *  - drop: pictures can be dropped on it (data-drop). mic, attach: its buttons (both by default; "composer" without
 *    `drop` has no attach button).
 *  - send: { attrs, label, disabled } for its Send. extra: buttons before Send (a "desk" box's, and "composer"'s
 *    always-shown `extra` with its `alt` already added when something's typed).
 *  - head: { label, cancel } puts a "desk" box under a title and a Cancel (.dk-sb, Send back, Go back); cancel is the
 *    Cancel button's attributes. sbClass: more classes for that .dk-sb.
 *  - "composer" only: bang (a "!" line typed), cmd (the "/" menu's slot), clock (the "Send at" button), below (under
 *    the row: the "Send at" bar and the "/" hint).
 *  Other looks, each the box of one place:
 *  - "mini": an answer in the mini panel's Desk row (.m-dk-box; rows 1): the words and Send, no mic or pictures.
 *    attrs: more of the textarea's attributes.
 *  - "btw": the by-the-way panel's question (.btw-ask; rows 1), "forward": the note on a forwarded message (.fwd-note;
 *    rows 2): just the words, Enter sends (app/keys.js).
 *  No hover text: say what a button does in its aria-label. */
function boxHtml(o) {
  const key = o.key, k = esc(key);
  const text = o.text ?? draft(key).text;
  const on = dictating?.key === key;
  const chips = o.chips === false ? "" : draftChips(key);
  const sug = o.sug || "";
  const send = o.send || {};
  if (o.look === "composer") {
    const ta = `<textarea rows="${o.rows || 2}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off" data-text="${k}"${sug ? ` class="sug"${o.sugSid ? ` data-sug-sid="${esc(o.sugSid)}"` : ""} aria-label="${esc(`Suggested: ${sug}. Tab types it in.`)}"` : ""} placeholder="${esc(sug ? " " : o.placeholder || "")}">${esc(text)}</textarea>`;
    const mic = o.mic === false ? "" : `<button class="icon-btn mic ${on ? "on" : ""}" data-mic="${k}" aria-label="Dictate">${MIC_ICON}</button>`;
    const attach = o.drop && o.attach !== false ? `<button class="icon-btn" data-attach="${k}" aria-label="Add image">${IMAGE_ICON}</button>` : "";
    return `<div class="composer${o.held ? " held" : ""}"${o.held ? " inert" : ""}><div class="field ${o.drop ? "drop" : ""} ${o.bang ? "bang" : ""}" data-drop="${o.drop ? k : ""}">${o.cmd || ""}${o.top || ""}${o.drop ? chips : ""}<div class="field-row">
    ${o.pre ? `<span class="field-to" data-desk-pre="">${Array.isArray(o.pre) ? `<span class="field-to1">${esc(o.pre[0])}</span><span class="field-to2">${esc(o.pre[1])}</span>` : esc(o.pre)}</span>` : ""}<div class="ta-wrap">${ta}${sugLineHtml(sug)}</div>
    ${mic}
    ${attach}
    ${o.clock || ""}
    ${o.extra || ""}<button class="btn primary send" ${send.attrs || ""} ${send.disabled ? "disabled" : ""}>${send.label ?? "Send"}</button></div>${o.below || ""}</div></div>`;
  }
  const placeholder = esc(sug ? " " : o.placeholder || "");
  if (o.look === "btw") return `<div class="btw-ask"><textarea data-text="${k}" rows="${o.rows || 1}" placeholder="${placeholder}" spellcheck="false">${esc(text)}</textarea></div>`;
  if (o.look === "forward") return `<textarea rows="${o.rows || 2}" class="fwd-note" spellcheck="false" data-text="${k}" placeholder="${placeholder}">${esc(text)}</textarea>`;
  const label = sug ? ` aria-label="${esc(`Suggested: ${sug}. Tab types it in.`)}"` : o.label ? ` aria-label="${esc(o.label)}"` : "";
  const fixed = 'spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off"';
  const mic = o.mic === false ? "" : `<button class="icon-btn mic ${on ? "on" : ""}" data-mic="${k}" aria-label="${on ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button>`;
  const attach = o.attach === false ? "" : `<button class="icon-btn" data-attach="${k}" aria-label="Add an image">${IMAGE_ICON}</button>`;
  const go = `<button class="btn primary small" ${send.attrs || ""}${send.disabled ? " disabled" : ""}>${send.label ?? "Send"}</button>`;
  if (o.look === "mini") return `<div class="m-dk-box"><textarea rows="${o.rows || 1}" data-text="${k}"${o.attrs ? ` ${o.attrs}` : ""}${label} placeholder="${placeholder}" ${fixed}>${esc(text)}</textarea>${go}</div>`;
   const ta = `<textarea rows="${o.rows || 1}" data-text="${k}"${sug ? ` class="sug"${o.sugSid != null ? ` data-sug-sid="${esc(o.sugSid)}"` : ""}` : ""}${label} placeholder="${placeholder}" ${fixed}>${esc(text)}</textarea>`;
  const field = o.wrap ? `<div class="ta-wrap">${ta}${sugLineHtml(sug)}</div>` : ta;
  const inner = `<div class="dk-box${o.head ? " dk-sb-box" : ""}"${o.drop === false ? "" : ` data-drop="${k}"`}>${o.top || ""}${chips}<div class="dk-box-row">${field}${mic}${attach}${o.extra || ""}${go}</div></div>`;
  if (!o.head) return inner;
  return `<div class="dk-sb${o.sbClass ? ` ${o.sbClass}` : ""}"><div class="dk-sb-h"><span class="dk-sb-l">${esc(o.head.label)}</span><button class="dk-sb-x" ${o.head.cancel || ""}>Cancel</button></div>${inner}</div>`;
}
