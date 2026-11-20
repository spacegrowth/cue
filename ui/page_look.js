// What the window looks like to the pointer and the keyboard right now, for when the cursor acts up (#456): asked
// over Cue's socket (`{"type":"page"}`), answered here. It only reads the page; nothing in it changes.
const pageLookPointer = { x: -1, y: -1, at: 0, buttons: 0, down: "", up: "" };

/** One element, short: its tag, id, classes and data- attribute names, and what makes it take or refuse the pointer. */
function pageLookEl(el) {
  if (!el || el.nodeType !== 1) return null;
  const cs = getComputedStyle(el);
  const cls = typeof el.className === "string" && el.className.trim() ? "." + el.className.trim().split(/\s+/).slice(0, 4).join(".") : "";
  const data = [...el.attributes].map((a) => a.name).filter((n) => n.startsWith("data-")).slice(0, 4);
  return {
    el: `${el.tagName.toLowerCase()}${el.id ? "#" + el.id : ""}${cls}${data.length ? `[${data.join(",")}]` : ""}`,
    cursor: cs.cursor, pointer_events: cs.pointerEvents, z: cs.zIndex, position: cs.position,
    editable: el.isContentEditable || (/^(INPUT|TEXTAREA)$/.test(el.tagName) && !el.readOnly && !el.disabled),
  };
}

/** The element and its parents, up to `n`. */
function pageLookChain(el, n = 6) {
  const out = [];
  for (let e = el; e && e.nodeType === 1 && out.length < n; e = e.parentElement) out.push(pageLookEl(e).el);
  return out;
}

/** Anything shown over at least half the window that takes the pointer (a forgotten overlay would). */
function pageLookCovers() {
  const area = innerWidth * innerHeight;
  return [...document.body.querySelectorAll("*")].filter((e) => {
    const cs = getComputedStyle(e);
    if (!/^(fixed|absolute)$/.test(cs.position) || cs.pointerEvents === "none" || cs.visibility === "hidden" || cs.display === "none") return false;
    const r = e.getBoundingClientRect();
    return r.width * r.height >= area / 2 && e !== document.getElementById("app");
  }).slice(0, 8).map(pageLookEl);
}

function pageLook() {
  const p = pageLookPointer;
  const under = p.x >= 0 ? document.elementFromPoint(p.x, p.y) : null;
  const sel = getSelection();
  return {
    has_focus: document.hasFocus(), visibility: document.visibilityState,
    active: pageLookEl(document.activeElement), active_chain: pageLookChain(document.activeElement),
    pointer: { x: p.x, y: p.y, moved_ms_ago: p.at ? Date.now() - p.at : null, buttons: p.buttons, last_down: p.down, last_up: p.up },
    under: pageLookEl(under), under_chain: pageLookChain(under),
    covers: pageLookCovers(),
    body: document.body.className, html: document.documentElement.className,
    selection: sel ? sel.type : "",
    window: { w: innerWidth, h: innerHeight },
  };
}

/** Where the pointer is and what it last pressed, kept as it moves (read only when asked). */
function pageLookWatch() {
  const at = (e) => { pageLookPointer.x = e.clientX; pageLookPointer.y = e.clientY; pageLookPointer.at = Date.now(); pageLookPointer.buttons = e.buttons; };
  addEventListener("pointermove", at, { capture: true, passive: true });
  addEventListener("pointerdown", (e) => { at(e); pageLookPointer.down = pageLookEl(e.target)?.el || ""; }, { capture: true, passive: true });
  addEventListener("pointerup", (e) => { at(e); pageLookPointer.up = pageLookEl(e.target)?.el || ""; }, { capture: true, passive: true });
}
pageLookWatch();

/** Cue asks (`page-look` with an id): the answer goes back under that id. */
async function listenPageLook(T) {
  await T.event.listen("page-look", (e) => {
    let look;
    try { look = pageLook(); } catch (err) { look = { error: String(err) }; }
    invoke("page_look_said", { id: e.payload.id, look }).catch(() => {});
  });
}
