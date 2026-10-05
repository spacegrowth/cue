#!/bin/bash
# Install Cue: the app, the Claude Code hooks, and the Pi extension.
#
#   ./install.sh            everything (app, Claude Code + Codex hooks, Pi extension)
#   ./install.sh --app      just the app
#   ./install.sh --usage    just "Claude usage in Cue" (the status-line pass-through below)
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

install_app() {
  if [ ! -d "$APP_SRC" ]; then
    say "building Cue.app (first time takes a few minutes)…"
    # An optional add-on in src-tauri/ext/ (not part of this repo) is built in when it's there.
    local ext=""; [ -e "$ROOT/src-tauri/ext/mod.rs" ] && ext="--features ext"
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

# Claude usage in Cue (asked during install): Claude Code hands its plan usage only to the
# status line command, so Cue puts a pass-through in front of yours. It forwards the numbers to
# Cue, then runs your own command unchanged (saved in statusline.orig); --uninstall puts it back.
statusline() {  # $1 = install | remove
  local wrapper="$CUE_HOME/bin/cue-statusline"
  if [ "$1" = install ]; then
    mkdir -p "$CUE_HOME/bin"
    cat > "$wrapper" <<'SH'
#!/bin/sh
# Cue's pass-through status line: hands Claude Code's usage (rate_limits) to Cue, then runs your
# own status line command unchanged. Put here by Cue's install.sh; --uninstall puts yours back.
input=$(cat)
dir=$(dirname "$0")
[ -x "$dir/cue-hook" ] && (printf '%s' "$input" | "$dir/cue-hook" usage claude >/dev/null 2>&1 &)
cmd=$(cat "$dir/../statusline.orig" 2>/dev/null)
[ -n "$cmd" ] && printf '%s' "$input" | sh -c "$cmd"
exit 0
SH
    chmod +x "$wrapper"
  fi
  python3 - "$SETTINGS" "$wrapper" "$CUE_HOME/statusline.orig" "$1" <<'EOF'
import json, os, shutil, sys, time
path, wrapper, orig, mode = sys.argv[1:]
try:
    cfg = json.load(open(path))
except Exception:
    cfg = {}
line = cfg.get("statusLine") or {}
ours = f'"{wrapper}"'
if mode == "install":
    if line.get("command") == ours:
        sys.exit(0)                       # already in place: keep the saved original
    open(orig, "w").write(line.get("command", ""))
    cfg["statusLine"] = {**line, "type": "command", "command": ours}
else:
    if line.get("command") != ours:
        sys.exit(0)                       # you've changed it since: leave yours alone
    prev = open(orig).read() if os.path.exists(orig) else ""
    if prev:
        cfg["statusLine"] = {**line, "command": prev}
    else:
        cfg.pop("statusLine", None)
if os.path.exists(path):
    shutil.copy(path, f"{path}.cue-backup-{time.strftime('%Y%m%d-%H%M%S')}")
with open(path, "w") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")
EOF
}

# Asked once, at install, only when someone's at the terminal to answer.
want_usage() {
  [ -t 0 ] || return 1
  printf '  Show Claude Code usage (5h / weekly) in Cue? It wraps your status line; nothing on screen changes. [Y/n] '
  read -r answer
  case "$answer" in n*|N*) return 1 ;; esac
}

uninstall() {
  statusline remove; say "status line restored"
  agent_hooks "$SETTINGS" claude remove; say "claude hooks removed (backup saved)"
  if [ -f "$CODEX_HOOKS" ]; then agent_hooks "$CODEX_HOOKS" codex remove; say "codex hooks removed (backup saved)"; fi
  rm -f "$PI_EXT_DIR/cue.ts"; say "pi extension removed"
  quit_cue
  rm -rf "$HOME/Applications/Cue.app" "$CUE_HOME/bin" "$OLD_HOME/bin"; say "app removed (your data is kept in $CUE_HOME)"
}

case "${1:-}" in
  --uninstall) uninstall ;;
  --app) install_app; open "$HOME/Applications/Cue.app" ;;
  --usage) statusline install; say "usage     → Claude Code's status line now also tells Cue (your own command still runs)" ;;
  "") install_app; install_hooks
      if want_usage; then statusline install; say "usage     → Claude Code's status line now also tells Cue (your own command still runs)"; fi
      open "$HOME/Applications/Cue.app"; say "done — new and running sessions pick up the hooks on their next event" ;;
  *) sed -n '2,9p' "$0"; exit 2 ;;
esac
