// A tmux server of the test's own. Every call names its socket (-S), and runs with TMUX and TMUX_PANE
// removed: an inherited $TMUX would otherwise point tmux at the server you work in, whatever else is set.
// Cue, launched with TMUX_TMPDIR set to the same folder and no $TMUX, finds this server as its "default":
// the socket below is exactly where it looks.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/** `env` without the variables that would send tmux, or anything started from here, to your own server. */
export function withoutTmux(env) {
  const out = { ...env };
  delete out.TMUX;
  delete out.TMUX_PANE;
  return out;
}

/** The sockets of tmux servers that aren't a test's: the one $TMUX names, and the default ones. */
function serversNotOurs() {
  const uid = process.getuid();
  const mine = [path.join("/private/tmp", `tmux-${uid}`, "default"), path.join("/tmp", `tmux-${uid}`, "default")];
  if (process.env.TMUX_TMPDIR) mine.push(path.join(process.env.TMUX_TMPDIR, `tmux-${uid}`, "default"));
  if (process.env.TMUX) mine.push(process.env.TMUX.split(",")[0]);
  return mine.map((p) => (fs.existsSync(p) ? fs.realpathSync(p) : path.resolve(p)));
}

export class Tmux {
  /** The server whose socket is in `dir` (a test's own folder, which exists already). */
  constructor(dir) {
    this.dir = dir;
    this.socket = path.join(dir, `tmux-${process.getuid()}`, "default");
    const real = path.join(fs.realpathSync(dir), `tmux-${process.getuid()}`, "default");
    if (serversNotOurs().includes(real)) throw new Error(`refusing to talk to a tmux server that isn't the test's: ${real}`);
    // What Cue and the agent stand-ins get: this folder as tmux's, and no $TMUX.
    this.env = withoutTmux({ PATH: process.env.PATH, TMUX_TMPDIR: dir, LANG: "en_US.UTF-8" });
  }

  run(...args) {
    return execFileSync("tmux", ["-S", this.socket, ...args], { env: this.env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  }

  /** A detached session running `command` (a /bin/sh line) with `env` added; its pane id. */
  newSession(name, command, env = {}) {
    fs.mkdirSync(path.dirname(this.socket), { recursive: true, mode: 0o700 });
    const vars = Object.entries(env).flatMap(([k, v]) => ["-e", `${k}=${v}`]);
    this.run("new-session", "-d", "-s", name, "-x", "120", "-y", "30", ...vars, "/bin/sh", "-c", command);
    return this.run("display-message", "-p", "-t", name, "#{pane_id}");
  }

  /** The pids of the clients attached to this server: on the test's own server, the built-in terminals
   *  Cue opened (each attaches through a view session of its own). */
  clients() {
    const out = this.run("list-clients", "-F", "#{client_pid}");
    return out ? out.split("\n").map(Number) : [];
  }

  /** Ends this server only (its socket named): never another. */
  killServer() {
    if (!fs.existsSync(this.socket)) return;
    try { execFileSync("tmux", ["-S", this.socket, "kill-server"], { env: this.env, stdio: "ignore" }); } catch {}
  }
}
