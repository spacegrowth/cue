// Cue UI app — typing into the window's fields, and pictures pasted or dropped into a box.

/** A field typed into: History's session picker, the quick phrases (saved half a second after you stop, not on
 *  "change": a redraw while you type swaps the field for a new one, and the new one never fires "change"), and
 *  every text box's draft, with what each box does as you type. */
function onInput(e) {
  if (e.target.matches?.("[data-hsession]")) { histSession = e.target.value; histOpen = null; return renderMain(); }
  if (e.target.matches?.("[data-quick-edit]")) { qpEdit = [...document.querySelectorAll("[data-quick-edit]")].map((i) => i.value); clearTimeout(qpSaveAt); qpSaveAt = setTimeout(saveQuickPhrases, 500); return; }
  const id = e.target.dataset?.text;
  if (!id) return;
  draft(id).text = e.target.value;
  if (id === "find-live") { liveSel = 0; return runSearch(e.target.value); }
  if (id === "find-sessions") return renderMain();
  if (id === "new-cwd") { nfFolderOpen = true; nfFolderQ = e.target.value; nfFolderSel = -1; saveDrafts(); return renderMain(); }
  if (id === "new-model-id") { newModel = e.target.value.replace(/\s+/g, ""); remember("newModel", newModel); remember("newModelOther", newModel); return; }
  saveDrafts();
  if (cmdShut === id) cmdShut = null;
  cmdSel = 0;
  refreshCmd(id);
  grow(e.target);
  const b = e.target.closest(".composer")?.querySelector(".btn.send");
  if (b && !b.dataset.held) b.disabled = !e.target.value.trim() && !draft(id).images.length;
}

/** The box a paste or drop lands in, if it takes pictures. */
const dropKey = (e) => e.target.closest?.("[data-drop]")?.dataset.drop;
/** Pictures pasted into a box. The web view sometimes hides a pasted image: when there's no text either, macOS
 *  is asked for it directly. */
async function onPaste(e) {
  const key = dropKey(e);
  if (!key) return;
  const cd = e.clipboardData;
  const files = [...(cd?.items || [])].filter((i) => i.kind === "file" && i.type.startsWith("image/")).map((i) => i.getAsFile()).filter(Boolean);
  if (files.length) { e.preventDefault(); return attach(key, files); }
  if (!cd?.getData("text/plain")) {
    e.preventDefault();
    const data = await invoke("clipboard_image");
    if (data) { draft(key).images.push({ name: "pasted image", mime: "image/png", data }); renderMain(); document.querySelector(`[data-text="${CSS.escape(key)}"]`)?.focus(); }
  }
}
/** Pictures dropped on a box. */
function onDrop(e) {
  const key = dropKey(e);
  if (!key) return;
  e.preventDefault();
  attach(key, [...(e.dataTransfer?.files || [])].filter((f) => f.type.startsWith("image/")));
}

/** A setting's field changed: History's keep count, the usage file, + New's model (Other…: the id you typed last
 *  time comes back, so a one-off pick of a listed model doesn't lose it), a quick phrase. */
function onChange(e) {
  if (e.target.matches("[data-keep-custom]")) setSetting("history.keep", Math.max(10, Math.round(+e.target.value || 0)));
  if (e.target.matches("[data-usage-file]")) setSetting("usage.file", e.target.value.trim());
  if (e.target.matches("[data-model]")) {
    newModel = e.target.value === OTHER ? remembered("newModelOther") || "" : e.target.value;
    remember("newModel", newModel); renderMain();
    return e.target.value === OTHER && document.querySelector('[data-text="new-model-id"]')?.focus();
  }
  if (e.target.matches("[data-quick-edit]")) { saveQuickPhrases(); qpEdit = null; }
}

/** Images: 📎 picks a file; paste and drag-and-drop work on any box that takes them. Leaving a quick phrase field
 *  saves it at once (Esc closing Settings, a click elsewhere). */
function bindInputs(app) {
  app.addEventListener("input", onInput);
  app.addEventListener("paste", onPaste);
  app.addEventListener("dragover", (e) => { if (dropKey(e)) e.preventDefault(); });
  app.addEventListener("drop", onDrop);
  app.addEventListener("focusout", (e) => { if (e.target.matches?.("[data-quick-edit]")) saveQuickPhrases(); });
  app.addEventListener("change", onChange);
}
