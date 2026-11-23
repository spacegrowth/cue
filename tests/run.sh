#!/bin/bash
# Cue's tests, headless, never touching your real Cue or stealing the window's focus.
#   tests/run.sh [card|unit|ui|e2e|soak [minutes]|all]      (all when none is named)
#   card  what a Desk item's session runs before it says done: only the tests for the files this checkout changed
#         against main (its branch's commits, plus what isn't committed yet). It says first which sets it picked and
#         why, by the map in tests/card-tests.map (tests/card-pick.js reads it; docs/dev/test-speed.md). Documents
#         only: nothing runs. Shared ground, or a file no line of the map matches: a "wide card", the full suite for
#         that side (unit for Rust, ui for the window).
#           tests/run.sh card --list [file…]   says what it would run and runs nothing; with files, for those files
#           tests/run.sh card file…            runs the tests for those files
#         A full Rust run from a Desk item's copy (.claude/worktrees/desk-<n>) is refused (here and in
#         scripts/test-runner.sh): it runs after landing. CUE_FULL_RUN=1 goes ahead anyway; a wide card sets it.
#   unit  Rust tests (the app and src-tauri/xtask), then the protocol tests against a private, silent Cue: its own socket (CUE_HOME), no
#         notifications, no windows (CUE_QUIET). Your real Cue keeps running untouched. Then scripts/release.sh's
#         gates against stand-ins (tests/release/gates.test.sh): nothing built or published.
#         And scripts/full-run.sh against stand-in steps (tests/release/full_run.test.sh).
#   ui    every node test (tests/*.test.js, tests/ui/*.test.js), then the real UI in headless Chromium on the fake
#         bridge (tests/ui: screens against their baselines, and interactions). Needs `npm ci` once.
#   e2e   the real app driven over WebDriver, a private Cue per journey (tests/e2e/run.sh, docs/dev/e2e.md).
#   soak  the long stress run (docs/dev/soak.md), 10 minutes unless told: the World soak (src-tauri/src/testing/soak)
#         and the e2e soak against the real debug app (tests/e2e/soak) side by side, then both judged on one page.
#         The last line out is the page's path; any detector's failure is a non-zero exit. CUE_SOAK_DIR: where the
#         run's files go (a new /tmp folder otherwise); CUE_CHAOS_*: the chaos knobs, for both.
set -eo pipefail   # a failed step ends the run here
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Nothing here may reach the tmux server you're in: a Cue or test started from a tmux pane would inherit it.
unset TMUX TMUX_PANE
# Nor say it runs in a session Cue hosts or drives (run from one, it would): hooks started from such a session tell Cue
# nothing, so every test of a hook would wait for what never comes.
unset CUE_HOSTED CUE_DRIVEN_BY CUE_HOOK_RECONNECT_FOR

# A Desk item's copy doesn't run every Rust test itself: its own tests go by `card`, the full run after landing.
in_desk_copy() {
  [[ "$ROOT" =~ /\.claude/worktrees/desk-[0-9]+$ ]]
}

unit() {
  if in_desk_copy && [ "${CUE_FULL_RUN:-}" != 1 ]; then
    echo "unit: this is a Desk item's copy, where the full Rust run is left to the run after landing. Run tests/run.sh card: it runs the tests for what you changed. (CUE_FULL_RUN=1 runs everything anyway.)" >&2
    return 1
  fi
  export CUE_HOME="$(mktemp -d /tmp/cue-test.XXXX)"
  export CUE_QUIET=1
  # The Cue this run built: from its CARGO_TARGET_DIR when it has one (copies that share target/ build over each other).
  export CUE_BIN="${CARGO_TARGET_DIR:-$ROOT/src-tauri/target}/debug/cue"
  # The protocol tests' stand-in agents, and the private Cue typing into them, on a tmux server of their own:
  # with $TMUX unset, a bare `tmux` would otherwise start sessions on your default server.
  export TMUX_TMPDIR="$CUE_HOME/tmux"
  mkdir -p "$TMUX_TMPDIR"
  (cd "$ROOT/src-tauri" && cargo test -q --workspace 2>&1 | grep -E "test result|FAILED|panicked" ; cargo build -q)
  # The private Cue (and the one test_protocol.py restarts) quits itself once this run is gone: a run killed before
  # end_unit could run (a SIGKILL skips the EXIT trap) once left its Cue running for days, until it grew to 54 GB (#764).
  export CUE_EXIT_WITH=$$
  "$CUE_BIN" >/dev/null 2>&1 &
  CUE_PID=$!
  trap end_unit EXIT
  for i in $(seq 1 50); do [ -S "$CUE_HOME/cue.sock" ] && break; sleep 0.2; done
  python3 "$ROOT/tests/test_protocol.py"
  "$ROOT/tests/release/gates.test.sh"
  "$ROOT/tests/release/full_run.test.sh"
  "$ROOT/tests/release/debug_only.test.sh"
}

# What the unit run started, and only that: its private Cue (by its pid), whatever else runs with its CUE_HOME
# (the hooks its tests ran), the private tmux server by its own socket, then the folder. Never another Cue built
# from this checkout (a dev Cue, hooks shimmed to it). Each step may find nothing left to end.
end_unit() {
  kill "$CUE_PID" 2>/dev/null || true
  ps -E -ww -x -o pid=,command= | awk -v me=$$ -v home="CUE_HOME=$CUE_HOME" '$1 != me && index($0 " ", home " ") { print $1 }' | xargs kill 2>/dev/null || true
  tmux -S "$TMUX_TMPDIR/tmux-$(id -u)/default" kill-server 2>/dev/null || true
  rm -rf "$CUE_HOME"
}

ui() {
  for t in "$ROOT"/tests/*.test.js "$ROOT"/tests/ui/*.test.js; do node "$t"; done
  [ -d "$ROOT/node_modules/@playwright/test" ] || { echo "ui: run npm ci first (Playwright isn't installed)"; return 1; }
  (cd "$ROOT" && npx playwright test -c tests/ui/playwright.config.mjs)
}

e2e() {
  "$ROOT/tests/e2e/run.sh"
}

# One step of a card's tests: said, then run from `$1` (under the top folder); with --list only said. A step that
# fails is said and counted (CARD_FAILED): the steps after it still run, so one run shows everything that is wrong.
card_step() {
  local dir="$1"
  shift
  if [ "$CARD_LIST" = 1 ]; then echo "would run: ${dir:+(in $dir) }$*"; return 0; fi
  echo "card: ${dir:+(in $dir) }$*" >&2
  (cd "$ROOT/$dir" && "$@") || { echo "card: FAILED: $*" >&2; CARD_FAILED=$((CARD_FAILED + 1)); return 1; }
}

card() {
  CARD_LIST=0
  CARD_FAILED=0
  if [ "${1:-}" = "--list" ]; then CARD_LIST=1; shift; fi
  local files=() f base
  if [ $# -gt 0 ]; then
    files=("$@")
  else
    base="$(git -C "$ROOT" merge-base HEAD main 2>/dev/null)" || { echo "card: no main branch to compare with here" >&2; return 2; }
    while IFS= read -r f; do [ -n "$f" ] && files+=("$f"); done < <(git -C "$ROOT" diff --no-renames --name-only "$base"; git -C "$ROOT" ls-files --others --exclude-standard)
  fi
  local plan
  plan="$(mktemp "${TMPDIR:-/tmp}/cue-card-plan.XXXXXX")"
  node "$ROOT/tests/card-pick.js" --plan "$plan" ${files[@]+"${files[@]}"} || { rm -f "$plan"; return 2; }
  local filters=() specs=() runs=() rust=0 rust_wide=0 window_wide=0 node_tests=0 check_window=0 step rest
  while IFS= read -r step; do
    rest="${step#* }"
    case "$step" in
      rust-wide) rust_wide=1 ;;
      window-wide) window_wide=1 ;;
      rust) rust=1 ;;
      rust-filter\ *) filters+=("$rest") ;;
      spec\ *) specs+=("$rest") ;;
      node-tests) node_tests=1 ;;
      check-window) check_window=1 ;;
      run\ *) runs+=("$rest") ;;
    esac
  done < "$plan"
  rm -f "$plan"
  local t
  # The window first: it is the shorter side.
  if [ "$window_wide" = 1 ]; then
    if [ "$CARD_LIST" = 1 ]; then echo "would run: tests/run.sh ui"; else ui; fi
  elif [ "$node_tests" = 1 ]; then
    for t in "$ROOT"/tests/*.test.js "$ROOT"/tests/ui/*.test.js; do card_step "" node "${t#"$ROOT"/}" || true; done
    if [ "$check_window" = 1 ] || [ ${#specs[@]} -gt 0 ]; then
      [ "$CARD_LIST" = 1 ] || [ -d "$ROOT/node_modules/@playwright/test" ] || { echo "card: run npm ci first (Playwright isn't installed)"; return 1; }
      card_step "" node scripts/check-window.mjs . || true
      card_step "" npx playwright test -c tests/ui/playwright.config.mjs "${specs[@]}" || true
    fi
  fi
  if [ "$rust_wide" = 1 ]; then
    if [ "$CARD_LIST" = 1 ]; then echo "would run: CUE_FULL_RUN=1 tests/run.sh unit"; else export CUE_FULL_RUN=1; unit; fi
  elif [ "$rust" = 1 ]; then
    # Nothing to test when it doesn't build; a finding of the code check doesn't stop the tests.
    if card_step src-tauri cargo build -q; then
      card_step src-tauri cargo run -q -p xtask -- check || true
      card_step src-tauri cargo test -q --lib -- "${filters[@]}" || true
    fi
  fi
  for t in ${runs[@]+"${runs[@]}"}; do card_step "" bash -c "$t" || true; done
  [ "$CARD_LIST" = 1 ] && return 0
  if [ "$CARD_FAILED" -gt 0 ]; then echo "card: $CARD_FAILED step(s) failed (each said above as \"card: FAILED\")" >&2; return 1; fi
  echo "card: all passed" >&2
}

# Process `$1` and everything it started, children first (so a parent's own trap still runs as it ends).
kill_tree() {
  local child
  for child in $(pgrep -P "$1"); do kill_tree "$child"; done
  kill "$1" 2>/dev/null || true
}

soak() {
  export CUE_SOAK_MINUTES="${1:-10}"
  local dir="${CUE_SOAK_DIR:-$(mktemp -d /tmp/cue-soak.XXXX)}" failed=0
  mkdir -p "$dir"
  rm -f "$dir/rust.json" "$dir/e2e.json"
  echo "soak: $CUE_SOAK_MINUTES min, its files in $dir" >&2
  # Both built first, one after the other, then run as built: a run that starts while the other still compiles
  # measures the compiler, not Cue. And two builds side by side clash: Cue's build reruns whenever .git/packed-refs
  # moves (any copy's landing moves it), and each build rewrites src-tauri/gen/schemas/acl-manifests.json (the e2e
  # build's has one more plugin), which the other may copy half written ("failed to parse plugin manifest map").
  local lib
  lib=$(cd "$ROOT/src-tauri" && cargo test -q --lib --no-run --message-format=json 2>"$dir/build.log" \
    | python3 -c 'import json,sys; print(next(m["executable"] for m in map(json.loads, sys.stdin) if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "cue_lib"))') \
    && (cd "$ROOT/src-tauri" && cargo build -q --features e2e --target-dir "${CARGO_TARGET_DIR:-target}/e2e") >>"$dir/build.log" 2>&1 \
    || { echo "soak: the build failed (see $dir/build.log)" >&2; return 1; }
  # Side by side: the gate takes the run's length, not twice it. Each one's output goes to its own log. The test
  # program runs through cargo's runner (a private copy, scripts/test-runner.sh), from where cargo test runs it.
  (cd "$ROOT/src-tauri" && CUE_SOAK_OUT="$dir/rust.json" "$ROOT/scripts/test-runner.sh" "$lib" --ignored testing::soak::soak --nocapture) >"$dir/rust.log" 2>&1 &
  local rust=$!
  CUE_E2E_BUILT=1 CUE_SOAK_OUT="$dir/e2e.json" "$ROOT/tests/e2e/run.sh" soak/soak.test.js >"$dir/e2e.log" 2>&1 &
  local e2e=$!
  # Interrupted: both runs end with it, each by its pid with everything it started (the e2e run's own trap
  # then ends its Cue and tmux server), never left soaking in the background.
  trap 'kill_tree $rust; kill_tree $e2e; exit 130' INT TERM HUP
  wait $e2e || failed=1
  wait $rust || failed=1
  trap - INT TERM HUP
  local runs=()
  for run in rust e2e; do
    if [ -f "$dir/$run.json" ]; then runs+=("$dir/$run.json"); else echo "soak: the $run soak measured nothing (see $dir/$run.log)" >&2; failed=1; fi
  done
  if [ ${#runs[@]} -gt 0 ]; then
    (cd "$ROOT/src-tauri" && cargo run -q -p xtask -- soak-report "${runs[@]}") >"$dir/report.txt" || failed=1
  fi
  cat "$dir/report.txt" 2>/dev/null || true
  echo "$dir/report.txt"
  return $failed
}

case "${1:-all}" in
  card) shift; card "$@" ;;
  unit) unit ;;
  ui) ui ;;
  e2e) e2e ;;
  soak) soak "$2" ;;
  all) ui; unit; e2e; soak ;;
  *) echo "usage: tests/run.sh [card|unit|ui|e2e|soak [minutes]|all]"; exit 2 ;;
esac
