// The real Cue UI (the design preview, ../preview.html) in a frame, for mocks that add to it: what they
// put in its page is put back after each of its redraws (it rebuilds #app's markup every time).
const MockApp = {
  /** Loads the preview into `frame`; resolves to its window and document once it's drawn. */
  load(frame, query = "") {
    return new Promise((done) => {
      frame.addEventListener("load", () => done({ win: frame.contentWindow, doc: frame.contentDocument }), { once: true });
      frame.src = `../preview.html${query}`;
    });
  },
  /** `place()` now and after every change to the app's markup. It must only add what's missing (its own
   *  additions are changes too: the next call finds them there and stops). */
  keep(doc, place) {
    new MutationObserver(place).observe(doc.getElementById("app"), { childList: true, subtree: true });
    place();
  },
  /** Styles for what a mock adds inside the app. */
  style(doc, css) { doc.head.insertAdjacentHTML("beforeend", `<style>${css}</style>`); },
  /** A terminal button in the app's header (kept there across redraws): by Search, or right after the
   *  Claude usage meter with `after: "usage"` (by Search when there's none). A click calls `onClick`.
   *  Returns `set({ pressed, live })`: on or off, and its badge: "run" (a pulsing dot), "ok" (✓), "bad" (✗), "". */
  headerButton({ win, doc }, onClick, { after } = {}) {
    const icon = MockApp.icon(win);
    const now = { pressed: false, live: "" };
    MockApp.style(doc, `
      [data-mock-term] { position: relative; }
      .top-btn.icon[aria-pressed="true"] { opacity: 1; background: color-mix(in srgb, var(--on-bg) 16%, transparent); }
      .mt-live { position: absolute; top: 3px; right: 2px; font: 700 10px var(--sans); line-height: 1; }
      .mt-live.run { width: 7px; height: 7px; border-radius: 50%; background: var(--sel); animation: mtpulse 1.2s ease-in-out infinite; }
      .mt-live.ok { color: var(--sel); } .mt-live.bad { color: #E8907F; }
      @keyframes mtpulse { 50% { opacity: .35; } }`);
    const apply = () => {
      const b = doc.querySelector("[data-mock-term]");
      if (!b) return;
      b.setAttribute("aria-pressed", String(now.pressed));
      const l = b.querySelector(".mt-live");
      l.className = `mt-live ${now.live}`;
      l.textContent = now.live === "ok" ? "✓" : now.live === "bad" ? "✗" : "";
    };
    MockApp.keep(doc, () => {
      if (doc.querySelector("[data-mock-term]")) return;
      const html = `<button class="top-btn icon" data-mock-term aria-label="Terminal (⌃\`)">${icon}<span class="mt-live"></span></button>`;
      const usage = after === "usage" && doc.querySelector(".top .usage-wrap");
      if (usage) usage.insertAdjacentHTML("afterend", html);
      else doc.querySelector('.top [data-act="open-search"]')?.insertAdjacentHTML("beforebegin", html);
      apply();
    });
    doc.addEventListener("click", (e) => { if (e.target.closest("[data-mock-term]")) { e.stopPropagation(); onClick(); } }, true);
    return (v) => { Object.assign(now, v); apply(); };
  },
  /** ⌃` anywhere, in the app or the mock's own page (not inside a terminal: its tabs handle it there). */
  hotkey({ doc }, fn) {
    const hot = (e) => { if (e.ctrlKey && e.key === "`" && !e.target.closest?.(".xterm")) { e.preventDefault(); e.stopPropagation(); fn(); } };
    doc.addEventListener("keydown", hot, true);
    document.addEventListener("keydown", hot);
  },
  /** Cue's terminal icon (ui/core.js), at `size`. */
  icon(win, size = 16) { return win.eval("TERM_ICON").replace(/width="\d+" height="\d+"/, `width="${size}" height="${size}"`); },
};
