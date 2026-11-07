// Loads the real UI on the fake bridge (bridge.js, loaded before this): the fakes, then the bridge started, then
// ui/index.html's stylesheets and scripts in its order, read from index.html itself so the list is kept once.
// ?still=1: no animations or transitions, so a screenshot catches the final frame.
window.CueUI = (() => {
  const here = new URL(".", document.currentScript.src);
  const ui = new URL("../../ui/", here);
  /** The fakes, one file per area of src-tauri/src (tests/ui/fakes/<area>.js). commands.test.js checks this list
   *  names every file there. */
  const FAKES = ["state", "chat", "steps", "mcp", "sessions", "terminal", "machines", "insights", "insights_tab", "settings", "extensions", "actions"];
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

  /** `start` starts the bridge (CueBridge.start) once the fakes are in, before the UI's first script runs. */
  async function load(start) {
    if (new URLSearchParams(location.search).has("still")) document.head.append(Object.assign(document.createElement("style"), { textContent: "*,*::before,*::after{animation:none!important;transition:none!important}" }));
    await Promise.all(FAKES.map((f) => script(new URL(`fakes/${f}.js`, here))));
    start();
    const index = new DOMParser().parseFromString(await (await fetch(new URL("index.html", ui))).text(), "text/html");
    await Promise.all([...index.querySelectorAll('link[rel="stylesheet"]')].map((l) => sheet(new URL(l.getAttribute("href"), ui))));
    await Promise.all([...index.querySelectorAll("script[src]")].map((s) => script(new URL(s.getAttribute("src"), ui))));
    done();
  }

  return { FAKES, load, ready };
})();
