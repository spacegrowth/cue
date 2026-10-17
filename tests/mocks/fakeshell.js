// A pretend terminal for the mocks: a zsh-like shell (line editing, history, Tab completion, a small
// file tree, a few commands) and an agent screen, each driving one xterm.js Terminal. Nothing runs:
// every command's output is made up here.
(() => {
  const ESC = "\x1b[";
  const c = { dim: `${ESC}2m`, b: `${ESC}1m`, r: `${ESC}0m`, red: `${ESC}31m`, grn: `${ESC}32m`, yel: `${ESC}33m`, blu: `${ESC}1;34m`, mag: `${ESC}35m`, cyn: `${ESC}36m` };
  const HOME = "/Users/me";
  const now = () => new Date().toString().replace(/ GMT.*/, "");

  // The file tree: a folder is an object, a file its text.
  const FS = {
    Users: { me: { code: {
      "agent-relay": {
        "README.md": "# agent-relay\n\nOne lead, many executors. `relay spawn` opens one, `relay:list` shows them.\n",
        "package.json": '{\n  "name": "relay",\n  "version": "1.4.0",\n  "scripts": { "test": "vitest run", "build": "tsc -p ." }\n}\n',
        ".gitignore": "node_modules\ndist\n",
        src: { "board.ts": "export function board(rows) {\n  const cols = ['name', 'spend', 'status'];\n  const total = rows.reduce((s, r) => s + r.spend, 0);\n  return { cols, total };\n}\n", "list.ts": "export const list = () => [];\n", "usage.ts": "export const spendOf = (log) => log.length;\n", "spawn.ts": "export async function spawn() {}\n" },
        test: { "board.test.ts": "import { board } from '../src/board';\n", "list.test.ts": "import { list } from '../src/list';\n" },
        docs: { "packets.md": "A packet is one unit of work.\n", "reports.md": "Each executor writes a report.\n" },
        node_modules: {},
      },
      "web-app": { "package.json": "{}\n", src: { pages: { "Cart.tsx": "const MAX_ITEMS = 25;\n" } } },
      "data-pipeline": { "pyproject.toml": "[project]\nname = \"pipeline\"\n", tests: {} },
    } } },
    tmp: {},
  };
  const COMMANDS = ["cat", "cd", "clear", "date", "echo", "exit", "git", "help", "history", "ls", "mkdir", "npm", "pwd", "sleep", "touch", "tree", "uname", "whoami"];
  const GIT_SUB = ["branch", "diff", "log", "status"];

  const norm = (cwd, p) => {
    if (!p || p === "~") return HOME;
    if (p.startsWith("~/")) p = HOME + p.slice(1);
    const parts = (p.startsWith("/") ? p : `${cwd}/${p}`).split("/");
    const out = [];
    for (const x of parts) { if (!x || x === ".") continue; if (x === "..") out.pop(); else out.push(x); }
    return "/" + out.join("/");
  };
  const at = (abs) => abs.split("/").filter(Boolean).reduce((n, k) => (n && typeof n === "object" ? n[k] : undefined), FS);
  const isDir = (n) => n && typeof n === "object";
  const pretty = (abs) => (abs === HOME ? "~" : abs.startsWith(HOME + "/") ? "~" + abs.slice(HOME.length) : abs);
  const cols = (names, width) => {
    const w = Math.max(...names.map((n) => n.replace(/\x1b\[[0-9;]*m/g, "").length)) + 2;
    const per = Math.max(1, Math.floor(width / w));
    const rows = [];
    for (let i = 0; i < names.length; i += per) rows.push(names.slice(i, i + per).map((n) => n + " ".repeat(Math.max(0, w - n.replace(/\x1b\[[0-9;]*m/g, "").length))).join("").trimEnd());
    return rows.join("\r\n");
  };
  const commonPrefix = (xs) => xs.reduce((a, b) => { let i = 0; while (i < a.length && a[i] === b[i]) i++; return a.slice(0, i); });

  /** A zsh-like shell in `term`, starting in `cwd`. `onTitle(t)` as what it runs changes; `onExit()` on exit / ⌃D. */
  class Shell {
    constructor(term, { cwd = HOME + "/code/agent-relay", onTitle = () => {}, onExit = () => {} } = {}) {
      Object.assign(this, { term, cwd, onTitle, onExit, buf: "", pos: 0, hist: [], hi: 0, busy: null, lastTab: false, code: 0 });
      // A folder the tree doesn't have (a session's, say): made, empty, so the shell can start there.
      cwd.split("/").filter(Boolean).reduce((n, k) => (isDir(n[k]) ? n[k] : (n[k] = {})), FS);
      term.write(`${c.dim}Last login: ${now()} on ttys004${c.r}\r\n`);
      this.prompt();
      this.title();
    }
    /** What it runs now, or null at its prompt (the tab then shows its own name). */
    title(t) { this.onTitle(t || null); }
    branch() { return this.cwd.startsWith(HOME + "/code/") ? (this.cwd.includes("web-app") ? "cart-limits" : "main") : ""; }
    promptText() {
      const b = this.branch();
      return `${c.grn}me@mac${c.r} ${c.blu}${pretty(this.cwd)}${c.r}${b ? ` ${c.mag}(${b})${c.r}` : ""} % `;
    }
    prompt() { this.term.write(this.promptText()); }
    redraw() {
      this.term.write(`\r${ESC}K${this.promptText()}${this.buf}`);
      const back = this.buf.length - this.pos;
      if (back > 0) this.term.write(`${ESC}${back}D`);
    }
    out(s = "") { this.term.write(s.replace(/\r?\n/g, "\r\n") + (s && !s.endsWith("\n") ? "\r\n" : "")); }
    /** Keys from xterm (or a key row): one call per chunk, which may be a paste. */
    input(d) {
      if (this.busy) {
        if (d === "\x03") { this.busy.cancel(); }
        return;
      }
      const tabbed = d === "\t";
      switch (d) {
        case "\r": return this.enter();
        case "\x7f": case "\b": if (this.pos > 0) { this.buf = this.buf.slice(0, this.pos - 1) + this.buf.slice(this.pos); this.pos--; this.redraw(); } break;
        case "\t": this.complete(); break;
        case "\x1b[A": if (this.hi > 0) { this.hi--; this.buf = this.hist[this.hi]; this.pos = this.buf.length; this.redraw(); } break;
        case "\x1b[B": if (this.hi < this.hist.length) { this.hi++; this.buf = this.hist[this.hi] || ""; this.pos = this.buf.length; this.redraw(); } break;
        case "\x1b[C": if (this.pos < this.buf.length) { this.pos++; this.term.write(d); } break;
        case "\x1b[D": if (this.pos > 0) { this.pos--; this.term.write(d); } break;
        case "\x1b[H": case "\x1bOH": case "\x01": this.pos = 0; this.redraw(); break;
        case "\x1b[F": case "\x1bOF": case "\x05": this.pos = this.buf.length; this.redraw(); break;
        case "\x03": this.term.write("^C\r\n"); this.buf = ""; this.pos = 0; this.prompt(); break;
        case "\x0c": this.term.clear(); this.term.write(`\r${ESC}K`); this.redraw(); break;
        case "\x15": this.buf = this.buf.slice(this.pos); this.pos = 0; this.redraw(); break;
        case "\x0b": this.buf = this.buf.slice(0, this.pos); this.redraw(); break;
        case "\x17": { const left = this.buf.slice(0, this.pos).replace(/\S+\s*$/, ""); this.buf = left + this.buf.slice(this.pos); this.pos = left.length; this.redraw(); break; }
        case "\x04": if (!this.buf) { this.term.write("exit\r\n"); this.onExit(); } break;
        case "\x1b": case "\x1b[Z": break;
        default:
          if (d.startsWith("\x1b")) break;
          // A paste with newlines: each line in turn.
          if (/[\r\n]/.test(d)) { d.split(/\r\n|\r|\n/).forEach((l, i, all) => { this.input(l); if (i < all.length - 1) this.input("\r"); }); return; }
          const t = d.replace(/[\x00-\x1f]/g, "");
          if (!t) break;
          this.buf = this.buf.slice(0, this.pos) + t + this.buf.slice(this.pos);
          this.pos += t.length;
          if (this.pos === this.buf.length) this.term.write(t); else this.redraw();
      }
      this.lastTab = tabbed;
    }
    /** Tab: the command (first word), git's subcommands, else a path. One match fills it in; several fill
     *  what they share, and list them when there's nothing more to fill. */
    complete() {
      const left = this.buf.slice(0, this.pos);
      const word = left.match(/(\S*)$/)[1];
      const first = !/\S\s/.test(left);
      let options, base = word, dirOf = "";
      if (first) options = COMMANDS.filter((x) => x.startsWith(word)).map((x) => ({ name: x, dir: false }));
      else if (/^git\s+\S*$/.test(left)) options = GIT_SUB.filter((x) => x.startsWith(word)).map((x) => ({ name: x, dir: false }));
      else {
        const slash = word.lastIndexOf("/");
        dirOf = slash >= 0 ? word.slice(0, slash + 1) : "";
        base = word.slice(slash + 1);
        const dir = at(norm(this.cwd, dirOf || "."));
        options = isDir(dir) ? Object.keys(dir).filter((k) => k.startsWith(base) && (base.startsWith(".") || !k.startsWith("."))).sort().map((k) => ({ name: k, dir: isDir(dir[k]) })) : [];
      }
      if (!options.length) return this.term.write("\x07");
      let add;
      if (options.length === 1) add = options[0].name.slice(base.length) + (options[0].dir ? "/" : " ");
      else add = commonPrefix(options.map((o) => o.name)).slice(base.length);
      if (add) {
        this.buf = this.buf.slice(0, this.pos) + add + this.buf.slice(this.pos);
        this.pos += add.length;
        return this.redraw();
      }
      this.term.write("\r\n" + cols(options.map((o) => (o.dir ? `${c.blu}${o.name}/${c.r}` : o.name)), this.term.cols) + "\r\n");
      this.redraw();
    }
    enter() {
      const line = this.buf.trim();
      this.term.write("\r\n");
      this.buf = ""; this.pos = 0;
      if (line) { this.hist.push(line); }
      this.hi = this.hist.length;
      if (!line) return this.prompt();
      const r = this.run(line);
      if (r && r.then) return;
      this.prompt();
    }
    /** Lines written over time (a command at work), ⌃C stops it. */
    slow(title, steps, code = 0) {
      let timer, i = 0;
      return new Promise((done) => {
        const finish = (c) => { this.busy = null; this.code = c; this.title(); this.prompt(); done(); };
        this.busy = { cancel: () => { clearTimeout(timer); this.term.write("^C\r\n"); finish(130); } };
        this.title(title);   // (after busy: whoever hears the title sees it at work)
        const next = () => {
          if (i >= steps.length) return finish(code);
          const [ms, text] = steps[i++];
          timer = setTimeout(() => { if (text != null) this.out(text); next(); }, ms);
        };
        next();
      });
    }
    run(line) {
      const [cmd, ...args] = line.split(/\s+/);
      const flags = args.filter((a) => a.startsWith("-")).join("");
      const paths = args.filter((a) => !a.startsWith("-"));
      switch (cmd) {
        case "help": return this.out(`A pretend shell for the mock. Try: ${COMMANDS.join(", ")}.\nTab completes commands and paths (twice lists them), ↑/↓ walk history, ⌃C stops, ⌃L clears, ⌃A/⌃E/⌃U/⌃W edit.`);
        case "pwd": return this.out(this.cwd);
        case "whoami": return this.out("me");
        case "date": return this.out(now());
        case "uname": return this.out(flags.includes("a") ? "Darwin mac.local 25.5.0 Darwin Kernel Version 25.5.0 arm64" : "Darwin");
        case "echo": return this.out(args.join(" ").replace(/^["']|["']$/g, "").replace(/\$HOME/g, HOME).replace(/\$PWD/g, this.cwd));
        case "clear": this.term.clear(); this.term.write(`\r${ESC}K`); return;
        case "history": return this.out(this.hist.map((h, i) => `${String(i + 1).padStart(5)}  ${h}`).join("\n"));
        case "exit": this.onExit(); return { then: true };
        case "cd": {
          const to = norm(this.cwd, paths[0] || "~");
          if (!at(to)) return this.out(`cd: no such file or directory: ${paths[0]}`);
          if (!isDir(at(to))) return this.out(`cd: not a directory: ${paths[0]}`);
          this.cwd = to; this.title(); return;
        }
        case "ls": {
          const targets = paths.length ? paths : ["."];
          const all = flags.includes("a");
          return targets.forEach((p) => {
            const n = at(norm(this.cwd, p));
            if (n === undefined) return this.out(`ls: ${p}: No such file or directory`);
            if (!isDir(n)) return this.out(p);
            const names = Object.keys(n).filter((k) => all || !k.startsWith(".")).sort();
            if (targets.length > 1) this.out(`${p}:`);
            if (flags.includes("l")) {
              this.out(`total ${names.length * 8}`);
              names.forEach((k) => this.out(`${isDir(n[k]) ? "drwxr-xr-x" : "-rw-r--r--"}  1 me  staff  ${String(isDir(n[k]) ? 128 : n[k].length).padStart(5)} Oct 11 09:4${k.length % 10} ${isDir(n[k]) ? `${c.blu}${k}${c.r}` : k}`));
            } else if (names.length) this.out(cols(names.map((k) => (isDir(n[k]) ? `${c.blu}${k}${c.r}` : k)), this.term.cols));
          });
        }
        case "tree": {
          const walk = (n, pre) => Object.keys(n).filter((k) => !k.startsWith(".") && k !== "node_modules").sort().forEach((k, i, a) => {
            const last = i === a.length - 1;
            this.out(`${pre}${last ? "└── " : "├── "}${isDir(n[k]) ? `${c.blu}${k}${c.r}` : k}`);
            if (isDir(n[k])) walk(n[k], pre + (last ? "    " : "│   "));
          });
          const n = at(norm(this.cwd, paths[0] || "."));
          if (!isDir(n)) return this.out(`${paths[0]} [error opening dir]`);
          this.out(`${c.blu}${paths[0] || "."}${c.r}`); return walk(n, "");
        }
        case "cat": {
          if (!paths.length) return this.out("cat: reading stdin isn't part of the mock");
          return paths.forEach((p) => { const n = at(norm(this.cwd, p)); this.out(n === undefined ? `cat: ${p}: No such file or directory` : isDir(n) ? `cat: ${p}: Is a directory` : n.replace(/\n$/, "")); });
        }
        case "touch": case "mkdir": return paths.forEach((p) => {
          const abs = norm(this.cwd, p), parent = at(abs.replace(/\/[^/]+$/, "") || "/"), name = abs.split("/").pop();
          if (!isDir(parent)) return this.out(`${cmd}: ${p}: No such file or directory`);
          if (cmd === "mkdir" && parent[name] !== undefined) return this.out(`mkdir: ${p}: File exists`);
          if (parent[name] === undefined) parent[name] = cmd === "mkdir" ? {} : "";
        });
        case "sleep": { const s = Math.min(30, parseFloat(paths[0]) || 1); return this.slow(`sleep ${paths[0] || 1}`, [[s * 1000, null]]); }
        case "git": {
          if (!this.branch()) return this.out("fatal: not a git repository (or any of the parent directories): .git");
          const sub = paths[0];
          if (sub === "status") return this.out(`On branch ${this.branch()}\nChanges to be committed:\n  (use "git restore --staged <file>..." to unstage)\n\t${c.grn}modified:   src/board.ts${c.r}\n\t${c.grn}modified:   test/board.test.ts${c.r}\n\nChanges not staged for commit:\n\t${c.red}modified:   README.md${c.r}`);
          if (sub === "log") return this.out([["a41c9e2", "Board: spend column, sorted high to low"], ["7be0d13", "relay:list prints spend"], ["29f5a80", "Usage log parser"], ["e01b7c4", "Packets carry their goal"]].map(([h, m], i) => `${c.yel}${h}${c.r}${i === 0 ? ` ${c.cyn}(HEAD -> ${this.branch()})${c.r}` : ""} ${m}`).join("\n"));
          if (sub === "branch") return this.out(`* ${c.grn}${this.branch()}${c.r}\n  board-spend\n  readme-pass`);
          if (sub === "diff") return this.out(`${c.b}diff --git a/README.md b/README.md${c.r}\n${c.cyn}@@ -1,3 +1,4 @@${c.r}\n # agent-relay\n${c.grn}+${c.r}\n${c.grn}+The board shows each executor's spend.${c.r}`);
          return this.out(`usage: git <command>   (the mock knows: ${GIT_SUB.join(", ")})`);
        }
        case "npm": {
          if (paths[0] === "test" || paths[0] === "t") return this.slow("npm test", [[120, `\n> relay@1.4.0 test\n> vitest run\n`], [500, ` ${c.grn}✓${c.r} test/list.test.ts ${c.dim}(6 tests) 41ms${c.r}`], [450, ` ${c.grn}✓${c.r} test/usage.test.ts ${c.dim}(12 tests) 88ms${c.r}`], [600, ` ${c.grn}✓${c.r} test/board.test.ts ${c.dim}(23 tests) 132ms${c.r}`], [300, `\n ${c.b}Test Files${c.r}  ${c.grn}3 passed${c.r} (3)\n      ${c.b}Tests${c.r}  ${c.grn}41 passed${c.r} (41)\n   ${c.dim}Duration${c.r}  1.42s\n`]]);
          if (paths[0] === "run" && paths[1] === "lint") return this.slow("npm run lint", [[120, `\n> relay@1.4.0 lint\n> eslint src\n`], [900, `${c.b}src/board.ts${c.r}\n  3:9  ${c.red}error${c.r}  'cols' is assigned a value but never used  no-unused-vars\n\n${c.red}✖ 1 problem (1 error, 0 warnings)${c.r}`]], 1);
          if (paths[0] === "run" && paths[1] === "build") return this.slow("npm run build", [[120, `\n> relay@1.4.0 build\n> tsc -p .\n`], [1400, null]]);
          return this.out(`npm ${paths[0] || ""}: the mock knows npm test, npm run lint and npm run build`);
        }
        case "claude": case "pi": case "codex": return this.out(`${cmd}: this session's agent is in its first tab`);
        default: return this.out(`zsh: command not found: ${cmd}`);
      }
    }
  }

  /** The agent's own screen (the session's first tab): a Claude Code-like prompt; what you send gets a
   *  made-up answer after a moment. */
  class Agent {
    constructor(term, { name = "agent-relay", onTitle = () => {} } = {}) {
      Object.assign(this, { term, name, onTitle, buf: "", busy: null });
      term.write(`${c.dim}╭──────────────────────────────────────────╮${c.r}\r\n${c.dim}│${c.r} ${c.yel}✻${c.r} Welcome to ${c.b}Claude Code${c.r}                  ${c.dim}│${c.r}\r\n${c.dim}│${c.r}   ${c.dim}cwd: ~/code/${name}${c.r}${" ".repeat(Math.max(0, 26 - name.length))}${c.dim}│${c.r}\r\n${c.dim}╰──────────────────────────────────────────╯${c.r}\r\n\r\n`);
      term.write(`${c.dim}>${c.r} Add a spend column to the board\r\n\r\n${c.dim}⏺${c.r} Read(src/usage.ts)\r\n  ⎿ Read 212 lines\r\n${c.dim}⏺${c.r} Update(src/board.ts)\r\n  ⎿ Updated with 48 additions and 3 removals\r\n${c.dim}⏺${c.r} Packet 3 is done and staged. Spend resets when an executor is\r\n  retired: keep it per session, or carry it over?\r\n\r\n`);
      this.prompt();
      onTitle("Claude");
    }
    prompt() { this.term.write(`${c.dim}>${c.r} `); }
    input(d) {
      if (this.busy) { if (d === "\x1b" || d === "\x03") { clearTimeout(this.busy); this.busy = null; this.term.write(`\r${ESC}K  ${c.red}⎿ Interrupted by user${c.r}\r\n\r\n`); this.prompt(); } return; }
      if (d === "\r") {
        const t = this.buf.trim(); this.buf = "";
        this.term.write("\r\n");
        if (!t) return this.prompt();
        if (t === "/clear") { this.term.clear(); this.term.write(`\r${ESC}K`); return this.prompt(); }
        this.term.write(`\r\n${c.yel}✻ Thinking…${c.r} ${c.dim}(esc to interrupt)${c.r}`);
        this.busy = setTimeout(() => {
          this.busy = null;
          this.term.write(`\r${ESC}K${c.dim}⏺${c.r} (mock) Got it: “${t}”. Nothing real runs here.\r\n\r\n`);
          this.prompt();
        }, 1200);
        return;
      }
      if (d === "\x7f") { if (this.buf) { this.buf = this.buf.slice(0, -1); this.term.write("\b \b"); } return; }
      if (d === "\t") return;   // (Claude Code's Tab: thinking on/off, not part of the mock)
      if (d.startsWith("\x1b")) return;
      const t = d.replace(/[\x00-\x1f]/g, "");
      this.buf += t; this.term.write(t);
    }
  }

  Shell.HOME = HOME;
  /** Where a new shell can start (the + menu): home, then each project in the tree. */
  Shell.FOLDERS = [{ label: "Home", path: HOME }, ...Object.keys(FS.Users.me.code).map((p) => ({ label: p, path: `${HOME}/code/${p}` }))];
  window.FakeShell = Shell;
  window.FakeAgent = Agent;
})();
