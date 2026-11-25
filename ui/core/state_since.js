// Cue's state, fetched as only what changed since the one this window has (#877, src-tauri/src/hub/state_since.rs).
// With hundreds of sessions the whole state is tens of MB, and every change sent it all. Cue numbers each state it
// hands out and keeps each window's last one; this keeps the same one here, untouched, and makes the next out of the
// patch Cue sends. The window is given a copy of it each time, as it was given a fresh state before: it changes
// what it's given (a name just typed, a Desk's setting just saved) until Cue's next says so.

const stateHeld = { v: null, s: null };

/** The patch node `node` (see state_since.rs) applied to `cur`: the value it's now. Changes `cur` in place. */
function statePatch(node, cur) {
  if (!node || typeof node !== "object") return cur;
  if ("=" in node) return node["="];
  if ("k" in node) {
    const key = node.k;
    const list = Array.isArray(cur) ? cur : [];
    const by = new Map(list.map((e) => [String(e?.[key]), e]));
    for (const [id, n] of Object.entries(node.c || {})) by.set(id, statePatch(n, by.get(id)));
    return (node.ids || list.map((e) => String(e?.[key]))).map((id) => by.get(id));
  }
  if ("n" in node) {
    const list = Array.isArray(cur) ? cur : [];
    list.length = node.n;
    for (const [i, n] of Object.entries(node.i || {})) list[+i] = statePatch(n, list[+i]);
    return list;
  }
  const obj = cur && typeof cur === "object" && !Array.isArray(cur) ? cur : {};
  for (const [k, n] of Object.entries(node.o || {})) obj[k] = statePatch(n, obj[k]);
  for (const k of node.d || []) delete obj[k];
  return obj;
}

/** Cue's state now, as the window's own copy: what changed since the last, or all of it when Cue can't say. */
async function fetchState() {
  let r = await invoke("get_state", { since: true, have: stateHeld.v });
  // Not an answer by number (a stand-in for Cue that hands the whole state): it's the state.
  if (!r || r.state_v == null) return r;
  // Another fetch here landed first: this one's patch is from a state this window no longer has.
  if (r.patch && r.from !== stateHeld.v && r.state_v !== stateHeld.v) r = await invoke("get_state", { since: true, have: null });
  if (r.full) {
    stateHeld.v = r.state_v;
    stateHeld.s = r.full;
  } else if (r.patch && r.from === stateHeld.v) {
    stateHeld.s = statePatch(r.patch, stateHeld.s);
    stateHeld.v = r.state_v;
    stateHeld.s.now_ms = r.now_ms;
  }
  return structuredClone(stateHeld.s);
}

if (typeof module !== "undefined") module.exports = { statePatch };
