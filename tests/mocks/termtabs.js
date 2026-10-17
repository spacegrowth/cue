// Tabs for the mock terminal: plain shells, and (in a session's terminal) the session's own screen
// first, the agent, which can't be closed. Each tab is its own xterm.js Terminal in its own element; the
// one on show is fitted to the slot. A page lays out the strip and the slot and calls in here.
(() => {
  const cssVar = (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  const esc = (s) => String(s).replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch]);

  class TermTabs {
    /** `strip`: where the tab buttons go; `slot`: where the terminals go; `opts.fontSize`; `opts.onKey(e)`:
     *  the page's own shortcuts (return false to keep a key from the terminal); `opts.mapData(d)`: a
     *  last look at what's typed (a sticky Ctrl, say) before the tab's shell gets it; `opts.onAdd(button)`:
     *  what + does (default: a shell where the open one is); `opts.onEmpty()`: the last tab closed;
     *  `opts.onChange()`: after any change to the tabs (one added, closed, picked, or what it runs);
     *  `opts.render(tabs, active)`: the strip's markup, in place of a row of tabs (keep data-tab,
     *  data-tab-x and data-tab-add on what picks, closes and adds); `opts.onDone(tab, what, code)`: a
     *  command finished (`what` it was, its exit code). */
    constructor(strip, slot, opts = {}) {
      Object.assign(this, { strip, slot, opts, tabs: [], active: null, seq: 0, shells: 0, running: new Map() });
      strip.addEventListener("click", (e) => {
        const x = e.target.closest("[data-tab-x]");
        if (x) { e.stopPropagation(); return this.close(x.dataset.tabX); }
        const plus = e.target.closest("[data-tab-add]");
        if (plus) { e.stopPropagation(); return this.opts.onAdd ? this.opts.onAdd(plus) : this.add("shell", { cwd: this.current()?.driver.cwd }); }
        const t = e.target.closest("[data-tab]");
        if (t) { e.stopPropagation(); this.show(t.dataset.tab); }
      });
      // Middle click closes a shell, as in a browser.
      strip.addEventListener("auxclick", (e) => { const t = e.target.closest("[data-tab]"); if (e.button === 1 && t) this.close(t.dataset.tab); });
      this.watch = new ResizeObserver(() => this.fit());
      this.watch.observe(slot);
    }
    /** A new tab, shown: "agent", or "shell" (in `cwd`, else the shell's own default). */
    add(kind, { label, cwd } = {}) {
      const id = `t${++this.seq}`;
      const host = document.createElement("div");
      host.className = "term-host";
      host.hidden = true;
      this.slot.appendChild(host);
      const term = new Terminal({
        fontFamily: cssVar("--mono") || "Menlo, monospace", fontSize: this.opts.fontSize || 12, lineHeight: 1.1, cursorBlink: true, scrollback: 5000, macOptionIsMeta: true,
        theme: { background: cssVar("--slab"), foreground: cssVar("--on-slab"), cursor: cssVar("--sel"), cursorAccent: cssVar("--slab"), selectionBackground: "rgba(210, 222, 150, 0.35)" },
      });
      const fit = new FitAddon.FitAddon();
      term.loadAddon(fit);
      host.hidden = false;
      term.open(host);
      try { fit.fit(); } catch {}
      host.hidden = true;
      const tab = { id, kind, title: label || (kind === "agent" ? "Claude" : "zsh"), term, fit, host, n: kind === "shell" ? ++this.shells : 0 };
      // The page's shortcuts first: ⌘T and the rest never reach the shell.
      term.attachCustomKeyEventHandler((e) => (this.opts.onKey ? this.opts.onKey(e, this) !== false : true));
      const onTitle = (t) => { tab.title = t || (tab.n > 1 ? `zsh ${tab.n}` : "zsh"); this.draw(); };
      tab.driver = kind === "agent"
        ? new FakeAgent(term, { onTitle })
        : new FakeShell(term, { onTitle, onExit: () => this.close(id), ...(cwd ? { cwd } : {}) });
      term.onData((d) => tab.driver.input(this.opts.mapData ? this.opts.mapData(d) : d));
      this.tabs.push(tab);
      this.show(id);
      return tab;
    }
    show(id) {
      const tab = this.tabs.find((t) => t.id === id);
      if (!tab) return;
      this.active = id;
      for (const t of this.tabs) t.host.hidden = t !== tab;
      this.draw();
      this.fit();
      tab.term.focus();
    }
    close(id) {
      const i = this.tabs.findIndex((t) => t.id === id);
      if (i < 0 || this.tabs[i].kind === "agent") return;
      const [t] = this.tabs.splice(i, 1);
      t.term.dispose();
      t.host.remove();
      if (!this.tabs.length) { this.active = null; this.draw(); return this.opts.onEmpty?.(); }
      if (this.active === id) this.show((this.tabs[i] || this.tabs[i - 1]).id);
      else this.draw();
    }
    /** ⌘1…9; 9 is always the last. */
    nth(n) { const t = n >= 9 ? this.tabs[this.tabs.length - 1] : this.tabs[n - 1]; if (t) this.show(t.id); }
    step(d) { const i = this.tabs.findIndex((t) => t.id === this.active); this.show(this.tabs[(i + d + this.tabs.length) % this.tabs.length].id); }
    current() { return this.tabs.find((t) => t.id === this.active); }
    /** Keys from outside the terminal (a key row): straight to the tab's shell, as if typed. */
    send(d) { const t = this.current(); if (t) { t.driver.input(d); t.term.focus(); } }
    /** The open tab's last line with text on it (for the closed strip). */
    lastLine() {
      const b = this.current()?.term.buffer.active;
      if (!b) return "";
      for (let y = b.baseY + b.cursorY; y >= 0; y--) { const l = (b.getLine(y)?.translateToString(true) || "").trim(); if (l) return l; }
      return "";
    }
    fit() { const t = this.current(); if (t) try { t.fit.fit(); } catch {} }
    draw() {
      // What was running and isn't now (on a tab still open): finished.
      const now = new Map(this.busy().map((t) => [t.id, t.title]));
      for (const [id, what] of this.running) {
        const t = this.tabs.find((x) => x.id === id);
        if (t && !now.has(id)) this.opts.onDone?.(t, what, t.driver.code || 0);
      }
      this.running = now;
      if (this.opts.render) { this.strip.innerHTML = this.opts.render(this.tabs, this.active); this.opts.onChange?.(); return; }
      this.strip.innerHTML = this.tabs.map((t) => `<button class="ttab ${t.kind}" role="tab" data-tab="${t.id}" aria-selected="${t.id === this.active}">`
        + `${t.kind === "agent" ? `<span class="ttab-dot" aria-hidden="true"></span>` : ""}<span class="ttab-name">${esc(t.title)}</span>`
        + `${t.kind === "shell" ? `<span class="ttab-x" data-tab-x="${t.id}" role="button" aria-label="Close ${esc(t.title)}">×</span>` : ""}</button>`).join("")
        + `<button class="ttab-add" data-tab-add aria-label="New shell tab">+</button>`;
      this.opts.onChange?.();
    }
    /** The tabs at work on something (a command running), for a live indicator. */
    busy() { return this.tabs.filter((t) => t.driver.busy); }
  }
  /** The tab shortcuts, as TermTabs' `onKey`: ⌘T a new shell where this one is, ⌘W close, ⌘1…9 pick,
   *  ⌃Tab / ⌃⇧Tab and ⌘⇧] / ⌘⇧[ next / previous, ⌃` `toggle()`. ⌥ works in place of ⌘, since a browser
   *  keeps ⌘T, ⌘W, ⌘1… and ⌃Tab for itself. */
  TermTabs.keys = (toggle) => (e, tt) => {
    if (e.type !== "keydown") return true;
    const mod = e.metaKey || e.altKey;
    const take = (f) => { e.preventDefault(); f(); return false; };
    if (mod && e.code === "KeyT") return take(() => tt.add("shell", { cwd: tt.current()?.driver.cwd }));
    if (mod && e.code === "KeyW") return take(() => tt.close(tt.active));
    if (mod && /^Digit[1-9]$/.test(e.code)) return take(() => tt.nth(+e.code.slice(5)));
    if (e.ctrlKey && e.key === "Tab") return take(() => tt.step(e.shiftKey ? -1 : 1));
    if (mod && e.shiftKey && (e.code === "BracketRight" || e.code === "BracketLeft")) return take(() => tt.step(e.code === "BracketRight" ? 1 : -1));
    if (e.ctrlKey && e.key === "`") return take(toggle);
    return true;
  };
  /** The + menu: where the new shell starts. `folders`: [{ label, path }]; `pick(path)`. One open at a time;
   *  a click elsewhere or Esc closes it. Styled by the page (.tmenu, .tmenu button). */
  TermTabs.menu = (anchor, folders, pick) => {
    document.querySelector(".tmenu")?.remove();
    const m = document.createElement("div");
    m.className = "tmenu";
    m.setAttribute("role", "menu");
    m.innerHTML = `<div class="tmenu-h">New shell in</div>` + folders.map((f, i) => `<button role="menuitem" data-i="${i}"><span>${esc(f.label)}</span><span class="tmenu-p">${esc(f.path.replace(/^\/Users\/[^/]+/, "~"))}</span></button>`).join("");
    document.body.appendChild(m);
    const r = anchor.getBoundingClientRect();
    m.style.left = `${Math.max(8, Math.min(r.left, innerWidth - m.offsetWidth - 8))}px`;
    m.style.top = r.top - m.offsetHeight - 6 > 8 ? `${r.top - m.offsetHeight - 6}px` : `${r.bottom + 6}px`;
    const done = () => { m.remove(); document.removeEventListener("pointerdown", away, true); document.removeEventListener("keydown", key, true); };
    const away = (e) => { if (!m.contains(e.target)) done(); };
    const key = (e) => { if (e.key === "Escape") { e.stopPropagation(); done(); } };
    m.addEventListener("click", (e) => { const b = e.target.closest("[data-i]"); if (b) { done(); pick(folders[+b.dataset.i].path); } });
    // (Opened by a click: its pointerdown has passed, so these see only what comes after.)
    document.addEventListener("pointerdown", away, true);
    document.addEventListener("keydown", key, true);
    m.querySelector("button")?.focus();
  };
  TermTabs.esc = esc;
  window.TermTabs = TermTabs;
})();
