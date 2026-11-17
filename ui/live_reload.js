// The window's files changed on the main line, and only they: Cue serves the new ones in place of its own (live_ui.rs)
// and says "ui-reload". The window loads them again once you aren't typing or pressing: 3 s after your last key or
// click, so your drafts are saved first (window.js saves them 0.3 s after a change). Sessions and the Desk run in Cue
// itself and carry on. In a block of its own, so its names never meet a page's.
{
  const QUIET_MS = 3000;
  let due = false, last = 0;
  const busy = () => Date.now() - last < QUIET_MS;
  for (const e of ["keydown", "pointerdown", "compositionupdate"]) addEventListener(e, () => { last = Date.now(); }, true);
  const reload = () => {
    if (!due) return;
    if (busy()) return setTimeout(reload, 1000);
    location.reload();
  };
  window.__TAURI__?.event?.listen("ui-reload", () => { due = true; reload(); });
}
