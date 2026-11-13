// Loads the real UI on the fake bridge (bridge.js, loaded before this): the fakes, then the bridge started, then
// ui/index.html's stylesheets and scripts in its order, read from index.html itself so the list is kept once (or
// another page's: mini.html for the mini window, tests/ui/mini.html).
// ?still=1: no animations or transitions, so a screenshot catches the final frame.
// ?addon=1: the optional add-on's part of the window too (src-tauri/ext/ui: its script and styles in place of the
// empty addon.js and styles/addon.css, as a build with it serves them, and its fakes in tests/fakes.js).
window.CueUI = (() => {
  const here = new URL(".", document.currentScript.src);
  const ui = new URL("../../ui/", here);
  const addon = new URLSearchParams(location.search).has("addon") ? new URL("../../src-tauri/ext/ui/", here) : null;
  /** Where the window's file `rel` (as index.html names it) comes from. */
  const from = (rel) => (addon && rel === "addon.js" ? new URL("addon.js", addon) : addon && rel === "styles/addon.css" ? new URL("addon.css", addon) : new URL(rel, ui));
  /** The fakes, one file per area of src-tauri/src (tests/ui/fakes/<area>.js). commands.test.js checks this list
   *  names every file there. */
  const FAKES = ["state", "chat", "steps", "mcp", "sessions", "terminal", "machines", "insights", "insights_tab", "settings", "extensions", "actions", "mini"];
  /** A script, run in the order it was added among the others added this way (async = false). */
  const script = (src) => new Promise((ok, fail) => {
    const s = Object.assign(document.createElement("script"), { src, async: false, onload: ok, onerror: () => fail(new Error(`couldn't load ${src}`)) });
    document.head.append(s);
  });
  const sheet = (href) => new Promise((ok, fail) => {
    const l = Object.assign(document.createElement("link"), { rel: "stylesheet", href, onload: ok, onerror: () => fail(new Error(`couldn't load ${href}`)) });
    document.head.append(l);
  });
  let done;
  const ready = new Promise((r) => (done = r));

  /** `start` starts the bridge (CueBridge.start) once the fakes are in, before the UI's first script runs. `page`:
   *  the page of ui/ whose stylesheets and scripts load. */
  async function load(start, page = "index.html") {
    if (new URLSearchParams(location.search).has("still")) document.head.append(Object.assign(document.createElement("style"), { textContent: "*,*::before,*::after{animation:none!important;transition:none!important}" }));
    await Promise.all(FAKES.map((f) => script(new URL(`fakes/${f}.js`, here))));
    if (addon) await script(new URL("tests/fakes.js", addon));
    start();
    const index = new DOMParser().parseFromString(await (await fetch(new URL(page, ui))).text(), "text/html");
    await Promise.all([...index.querySelectorAll('link[rel="stylesheet"]')].map((l) => sheet(from(l.getAttribute("href")))));
    await Promise.all([...index.querySelectorAll("script[src]")].map((s) => script(from(s.getAttribute("src")))));
    done();
  }

  return { FAKES, load, ready };
})();
