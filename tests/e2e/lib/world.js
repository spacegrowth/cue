// One private Cue per journey: the debug build with the e2e feature, its own folders (CUE_HOME, HOME, the
// tmux server's), quiet (CUE_QUIET: no notifications, no login item, its window hidden), and the
// WebDriver session that drives its real window. Nothing here reaches your Cue, your tmux or your home.
import { spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";
import { waitFor } from "./poll.js";
import { call, tell } from "./socket.js";
import { Tmux, withoutTmux } from "./tmux.js";
import { Window } from "./window.js";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const BIN = process.env.CUE_E2E_BIN || path.join(REPO, "src-tauri/target/e2e/debug/cue");

/** A port nobody listens on right now, for the app's WebDriver server. */
function freePort() {
  return new Promise((resolve, reject) => {
    const s = net.createServer().listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => resolve(port));
    });
    s.on("error", reject);
  });
}

export class World {
  /** A fresh world for test `name`; `launch()` starts its Cue. */
  constructor(name) {
    // Short: the socket's path ($CUE_HOME/cue.sock, and tmux's in TMUX_TMPDIR) must fit a Unix socket
    // address (104 bytes on macOS).
    const parent = process.env.CUE_E2E_ROOT || os.tmpdir();
    this.root = fs.mkdtempSync(path.join(parent, `${name.replace(/\W+/g, "-").slice(0, 12)}-`));
    if (path.join(this.root, "tmux/tmux-000/default").length > 100) throw new Error(`${this.root}: too long a path for a socket`);
    this.cueHome = path.join(this.root, "cue");
    this.home = path.join(this.root, "home");
    for (const d of [this.cueHome, this.home, path.join(this.root, "tmux")]) fs.mkdirSync(d);
    this.sock = path.join(this.cueHome, "cue.sock");
    this.tmux = new Tmux(path.join(this.root, "tmux"));
    this.children = new Set();
    this.app = null;
    this.ui = null;
  }

  /** The environment of the test's Cue and of every agent stand-in it runs. */
  env(extra = {}) {
    // Built from nothing, not from this process's environment; withoutTmux says so for the one thing
    // that must never come along: a $TMUX naming the server you work in.
    return withoutTmux({ PATH: process.env.PATH, LANG: "en_US.UTF-8", HOME: this.home, CUE_HOME: this.cueHome, CUE_QUIET: "1",
      TMUX_TMPDIR: this.tmux.dir, ...extra });
  }

  /** A child process to end with the world. */
  track(proc) {
    this.children.add(proc);
    proc.on("exit", () => this.children.delete(proc));
    return proc;
  }

  /** Start Cue and wait until its socket answers and its window has drawn its first state. */
  async launch() {
    const port = await freePort();
    const log = fs.openSync(path.join(this.root, "cue.log"), "a");
    this.app = spawn(BIN, [], { env: this.env({ TAURI_WEBDRIVER_PORT: String(port) }), stdio: ["ignore", log, log] });
    fs.closeSync(log);
    const app = this.app;
    this.exited = new Promise((resolve) => app.on("exit", resolve));
    await waitFor(() => this.alive() && call(this.sock, { type: "list" }, 2000), "Cue's socket", { timeout: 30_000, every: 100 });
    const browser = await waitFor(() => remote({ hostname: "127.0.0.1", port, logLevel: "silent", connectionRetryCount: 0,
      capabilities: { browserName: "tauri" } }), "Cue's WebDriver server", { timeout: 30_000, every: 100 });
    this.ui = new Window(browser);
    await this.ui.waitUntil(() => typeof state === "object" && Object.keys(state.settings || {}).length > 0, "the window's first state", 30_000);
  }

  alive() {
    return this.app && this.app.exitCode === null && this.app.signalCode === null;
  }

  /** End Cue with SIGTERM and wait until it is gone: a relaunch then reads only what Cue had saved. */
  async quit() {
    await this.ui?.close();
    this.ui = null;
    if (this.alive()) {
      this.app.kill("SIGTERM");
      await this.exited;
    }
  }

  async relaunch() {
    await this.quit();
    await this.launch();
  }

  /** Ask Cue over its socket, as cue-ctl does. */
  call(msg) {
    return call(this.sock, msg);
  }

  /** Report an event to Cue, as an agent's hook does. */
  tell(msg) {
    return tell(this.sock, msg);
  }

  async list() {
    return this.call({ type: "list" });
  }

  /** Everything ended: the window's session, Cue, every stand-in, the tmux server. The folders stay when
   *  `keep` (a failed journey: its log and screenshot are in there). */
  async stop({ keep = false } = {}) {
    await this.ui?.close();
    for (const p of this.children) p.kill("SIGKILL");
    if (this.alive()) {
      this.app.kill("SIGKILL");
      await this.exited;
    }
    this.tmux.killServer();
    if (!keep) fs.rmSync(this.root, { recursive: true, force: true });
  }
}
