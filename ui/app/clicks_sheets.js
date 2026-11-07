// Cue UI app — clicks in the sheets and what's set up there: first launch's setup, updates, search and a found
// conversation, add-on and setting buttons, and a box's pictures. Each list is a run of ui/app/clicks.js's order
// (first match wins).

/** Connect every agent that's installed and not yet connected, one at a time. */
async function connectAll() {
  const todo = ["claude", "codex", "pi"].filter((h) => { const c = state.connections?.[h]; return c && !c.ok && c.present; });
  for (const h of todo) { try { toast(await invoke("connect_agent", { harness: h })); } catch (e) { toast(`Couldn't connect ${h}: ${e}`); } }
}
const SETUP_CLICKS = [
  rule("[data-setup-done]", () => { sheet = null; setSetting("setup.done", true); renderMain(); }),
  rule("[data-connect-all]", () => { connectAll(); }),
];

/** The update card's buttons: look again, install it, or put it off. */
function updateClick(ub) {
  const a = ub.dataset.upd;
  if (a === "check") checkUpdate(); else if (a === "install") installUpdate(); else { upd = { ...upd, offer: false }; renderMain(); }
}
const UPDATE_CLICKS = [
  rule("[data-connect]", (cb) => { cb.disabled = true; invoke("connect_agent", { harness: cb.dataset.connect }).then((m) => toast(m), (e) => { toast(`Couldn't connect: ${e}`); cb.disabled = false; }); }),
  rule("[data-upd]", updateClick),
];

const FIND_CLICKS = [
  // An ended Claude Code session, opened from search: back to work, as an older one resumes.
  rule("[data-act=convo-resume]", () => resumeOlder({ session_id: convo.sid, name: convo.name })),
  rule("[data-act=convo-close]", () => { sheet = null; renderMain(); }),
  rule("[data-act=convo-back]", () => openFind()),
  rule("[data-act=convo-open]", (co) => { sheet = null; setActive(null, co.dataset.sid); }),
  rule("[data-srmore]", (sm) => { searchMore.add(sm.dataset.srmore); renderMain(); document.querySelector('[data-text="find-live"]')?.focus(); }),
  // Resume on a closed session's row: back to work at once (the row itself opens its conversation).
  rule("[data-resume]", (rsm) => resumeOlder({ session_id: rsm.dataset.resume, name: rsm.dataset.name })),
  rule("[data-sr]", (sr) => pickResult(+sr.dataset.sr)),
  rule("[data-star]", (starEl) => toggleStar(starEl.dataset.star)),
];

const SETTING_CLICKS = [
  rule("button[data-ext-pick]", (xp) => setExtPick(xp.dataset.extPick, xp.dataset.val)),
  rule("[data-ext-open]", (xo) => { extPickOpen = extPickOpen === xo.dataset.extOpen ? null : xo.dataset.extOpen; renderMain(); }),
  rule("[data-set]", (setBtn) => setSetting(setBtn.dataset.set, JSON.parse(setBtn.dataset.val))),
  // 📎 on a box: Finder's picker, for that box.
  rule("[data-attach]", (at) => { picker.dataset.key = at.dataset.attach; picker.value = ""; picker.click(); }),
  rule("[data-unattach]", (un) => { const [key, n] = un.dataset.unattach.split(/:(?=\d+$)/); draft(key).images.splice(+n, 1); renderMain(); }),
];
