#!/bin/bash
# Install Cue: the app, the Claude Code hooks, and the Pi extension.
#
#   ./install.sh            everything (app, Claude Code + Codex hooks, Pi extension)
#   ./install.sh --app      just the app
#   ./install.sh --uninstall
#
# Safe to re-run. Your ~/.claude/settings.json is backed up before it's touched, and only
# Cue's own hook entries are added or removed — your other hooks stay as they are.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
CUE_HOME="${CUE_HOME:-$HOME/Library/Application Support/dev.spacegrowth.cue}"
OLD_HOME="$HOME/.cue"   # where Cue lived before; kept only for forwarders
SETTINGS="$HOME/.claude/settings.json"
CODEX_HOOKS="$HOME/.codex/hooks.json"
PI_EXT_DIR="$HOME/.pi/agent/extensions"
APP_SRC="$ROOT/src-tauri/target/release/bundle/macos/Cue.app"

say() { printf '  %s\n' "$*"; }

agent_hooks() {  # $1 = file, $2 = claude|codex, $3 = remove (connecting is the app's: cue connect)
  python3 - "$1" "$CUE_HOME/bin/cue-hook" "$2" "$3" <<'EOF'
import json, os, re, shutil, sys, time
path, hook, harness, mode = sys.argv[1:5]
cfg = json.load(open(path)) if os.path.exists(path) else {}
if os.path.exists(path):
    shutil.copy(path, f"{path}.cue-backup-{time.strftime('%Y%m%d-%H%M%S')}")
hooks = cfg.setdefault("hooks", {})
mine = re.compile(r"cue-(claude-)?hook")
# Drop any earlier Cue entries, keep everything else.
for event, groups in list(hooks.items()):
    kept = []
    for g in groups:
        g["hooks"] = [h for h in g.get("hooks", []) if not mine.search(h.get("command", ""))]
        if g["hooks"]:
            kept.append(g)
    hooks[event] = kept
    if not kept:
        del hooks[event]
if not hooks:
    cfg.pop("hooks", None)
os.makedirs(os.path.dirname(path), exist_ok=True)
with open(path, "w") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")
EOF
}

# Ask Cue to quit and wait until it has (opening a new copy mid-shutdown fails with error -600).
quit_cue() {
  osascript -e 'quit app "Cue"' 2>/dev/null || true
  # "cue$": the app itself. Hooks run the same binary with arguments ("cue hook …") and must
  # survive (an agent may be waiting on one for your answer).
  for _ in $(seq 1 50); do pgrep -q -f "Cue.app/Contents/MacOS/cue$" || return 0; sleep 0.1; done
  pkill -f "Cue.app/Contents/MacOS/cue$" || true
}

# Start the installed Cue and make sure it is running. Right after a quit-and-replace, Launch
# Services can refuse the first open (error -609, "connection invalid") and the app silently stays
# down, so the open is retried until the process is seen.
launch_cue() {
  for _ in 1 2 3 4 5 6; do
    open "$HOME/Applications/Cue.app" 2>/dev/null || true
    for _ in $(seq 1 20); do
      pgrep -q -f "Cue.app/Contents/MacOS/cue$" && { say "Cue is running"; return 0; }
      sleep 0.25
    done
  done
  echo "  Cue did not start — open ~/Applications/Cue.app yourself (or run: open ~/Applications/Cue.app)" >&2
  return 1
}

install_app() {
  if [ ! -d "$APP_SRC" ]; then
    say "building Cue.app (first time takes a few minutes)…"
    # An optional add-on in src-tauri/ext/ (not part of this repo) is built in when it's there.
    local ext=""; [ -e "$ROOT/src-tauri/ext/mod.rs" ] && ext="--features ext"
    # Its own bundle settings (helper programs it ships beside Cue), when it has them.
    [ -n "$ext" ] && [ -e "$ROOT/src-tauri/ext/tauri.conf.json" ] && ext="$ext --config src-tauri/ext/tauri.conf.json"
    (cd "$ROOT" && npm install --silent && npx tauri build --bundles app $ext >/dev/null)
  fi
  quit_cue
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/Cue.app"
  cp -R "$APP_SRC" "$HOME/Applications/Cue.app"
  say "app       → ~/Applications/Cue.app"
}

install_hooks() {
  mkdir -p "$CUE_HOME/bin"
  # The hook is built into Cue's binary. This shim is what the agents' settings run; Cue rewrites
  # it on every launch (so moving the app can't break it), and writing it here too means hooks
  # work before Cue's first launch.
  printf '#!/bin/sh\nexec "%s" hook "$@"\n' "$HOME/Applications/Cue.app/Contents/MacOS/cue" > "$CUE_HOME/bin/cue-hook"
  cp "$ROOT/hooks/cue-ctl" "$CUE_HOME/bin/"   # optional shell tool for developers (needs python3)
  chmod +x "$CUE_HOME/bin/"*
  # Sessions started before Cue moved to Application Support (or before the cue-hook rename)
  # still call ~/.cue/bin/…: forward those to the real hook.
  mkdir -p "$OLD_HOME/bin"
  printf '#!/bin/sh\nexec "%s/bin/cue-hook" "$@"\n' "$CUE_HOME" > "$OLD_HOME/bin/cue-hook"
  printf '#!/bin/sh\nexec "%s/bin/cue-hook" "$1" claude\n' "$CUE_HOME" > "$OLD_HOME/bin/cue-claude-hook"
  printf '#!/bin/sh\nexec "%s/bin/cue-ctl" "$@"\n' "$CUE_HOME" > "$OLD_HOME/bin/cue-ctl"
  chmod +x "$OLD_HOME/bin/"*
  [ -f "$CUE_HOME/config.json" ] || echo '{ "pi": { "gate": "dangerous" } }' > "$CUE_HOME/config.json"
  # The app connects the agents itself (the same as its Settings → Connect).
  "$HOME/Applications/Cue.app/Contents/MacOS/cue" connect claude >/dev/null
  say "claude    → hooks in ~/.claude/settings.json (backup saved next to it)"
  if command -v codex >/dev/null || [ -d "$HOME/.codex" ]; then
    "$HOME/Applications/Cue.app/Contents/MacOS/cue" connect codex >/dev/null
    say "codex     → hooks in ~/.codex/hooks.json — in Codex, run /hooks once to trust them"
  fi
  if command -v pi >/dev/null; then
    mkdir -p "$PI_EXT_DIR"
    ln -sf "$ROOT/pi/cue.ts" "$PI_EXT_DIR/cue.ts"
    say "pi        → ~/.pi/agent/extensions/cue.ts (gate: see config.json in $CUE_HOME)"
  fi
}

# Cue's status line (Cue puts it in front of yours when Claude Code is connected; config.rs): on
# --uninstall, yours goes back (saved in statusline.orig). Left alone if you've changed it since.
statusline_remove() {
  python3 - "$SETTINGS" "$CUE_HOME/statusline.orig" <<'EOF'
import json, os, shutil, sys, time
path, orig = sys.argv[1:]
try:
    cfg = json.load(open(path))
except Exception:
    sys.exit(0)
line = cfg.get("statusLine") or {}
if "cue-statusline" not in line.get("command", ""):
    sys.exit(0)
prev = open(orig).read() if os.path.exists(orig) else ""
if prev:
    cfg["statusLine"] = {**line, "command": prev}
else:
    cfg.pop("statusLine", None)
shutil.copy(path, f"{path}.cue-backup-{time.strftime('%Y%m%d-%H%M%S')}")
with open(path, "w") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")
EOF
}

uninstall() {
  statusline_remove; say "status line restored"
  agent_hooks "$SETTINGS" claude remove; say "claude hooks removed (backup saved)"
  if [ -f "$CODEX_HOOKS" ]; then agent_hooks "$CODEX_HOOKS" codex remove; say "codex hooks removed (backup saved)"; fi
  rm -f "$PI_EXT_DIR/cue.ts"; say "pi extension removed"
  quit_cue
  rm -rf "$HOME/Applications/Cue.app" "$CUE_HOME/bin" "$OLD_HOME/bin"; say "app removed (your data is kept in $CUE_HOME)"
}

case "${1:-}" in
  --uninstall) uninstall ;;
  --app) install_app; launch_cue ;;
  "") install_app; install_hooks
      launch_cue; say "done — new and running sessions pick up the hooks on their next event" ;;
  *) sed -n '2,9p' "$0"; exit 2 ;;
esac
