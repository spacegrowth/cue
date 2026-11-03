// The status list an agent ends its replies with, and how the chat shows it. The session writes:
//   **Where we are**
//   ✓ done   → now   ○ left
//   Waiting on you: <a decision, or "nothing">      (older replies: "⏸ Waiting on you: …")
// Only the latest reply in a chat keeps it, folded to one line; older replies drop the block. The rules are
// kept in this one place.

const STATUS_HEADING = /^[ \t]*\*\*Where we are\*\*:?[ \t]*$/gm;
const STATUS_MARKS = { "✓": "done", "→": "now", "○": "left" };
const STATUS_WAITING = /^(?:⏸\s*)?Waiting on you:\s*(.*)$/i;
const STATUS_NOTHING = /^(?:nothing|none)\.?$/i;

/** A reply split into its words and its status list: { body, items: [{ state, text }], waiting, now } or, when it
 *  doesn't end with a list (none, or more writing after it), { body: text, items: [] }. `waiting`: what it waits on,
 *  "" for nothing. `now`: the item it's on (the first →, else the first ○). */
function parseStatus(text) {
  const reply = String(text ?? "");
  const none = { body: reply, items: [], waiting: "", now: null };
  const at = [...reply.matchAll(STATUS_HEADING)].pop()?.index;
  if (at == null) return none;
  const items = [];
  let waiting = "";
  for (const raw of reply.slice(at).split("\n").slice(1)) {
    const line = raw.trim();
    if (!line) continue;
    const state = STATUS_MARKS[line[0]];
    const wait = STATUS_WAITING.exec(line);
    if (state) items.push({ state, text: line.slice(1).trim() });
    else if (wait) waiting = STATUS_NOTHING.test(wait[1].trim()) ? "" : wait[1].trim();
    else return none;   // writing after the list: not its status list
  }
  if (!items.length) return none;
  const body = reply.slice(0, at).trimEnd().replace(/(?:^|\n)[ \t]*-{3,}$/, "").trimEnd();
  return { body, items, waiting, now: items.find((i) => i.state === "now") || items.find((i) => i.state === "left") || null };
}
/** The reply without its status list (for every reply but the latest, and for reading aloud). */
const stripStatus = (text) => parseStatus(text).body;

/** The one-line status under the latest reply (a segment per item, the item it's on or what it waits on, how many
 *  are done, ▸), and opened (`open`) the whole list as plain lines. `key` says which is open (openAsks). */
function statusHtml(status, key, open) {
  const { items, waiting, now } = status;
  if (!items.length) return "";
  const done = items.filter((i) => i.state === "done").length;
  const bar = items.map((i) => `<i class="st-seg ${i.state}"></i>`).join("");
  const here = waiting ? `Waiting on you: ${esc(waiting)}` : esc(now ? now.text : "All done");
  const head = `<button class="st-line" data-ask-open="${esc(key)}" aria-expanded="${open}"><span class="st-bar">${bar}</span><span class="st-now">${here}</span><span class="st-count">· ${done} of ${items.length} done</span><span class="st-more">${open ? "▾" : "▸"}</span></button>`;
  const mark = (state) => Object.keys(STATUS_MARKS).find((m) => STATUS_MARKS[m] === state);
  const full = open ? `<div class="st-list">${items.map((i) => `<div class="st-item ${i.state}"><span class="st-mark">${mark(i.state)}</span>${esc(i.text)}</div>`).join("")}<div class="st-wait">Waiting on you: ${esc(waiting || "nothing")}</div></div>` : "";
  return `<div class="cv-status">${head}${full}</div>`;
}

if (typeof module !== "undefined") module.exports = { parseStatus, stripStatus, statusHtml };
