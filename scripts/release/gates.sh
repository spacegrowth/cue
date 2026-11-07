# The gates a release passes before anything is built (sourced by scripts/release.sh; docs/dev/releasing.md).
# Each gate's output goes to its own log; the first gate that fails stops the release. Gate functions run as an
# `if` condition, where `set -e` doesn't apply, so each one chains its steps with && or returns its own status.

SOAK_MINUTES=10
GATES_PASSED=""   # the gates passed so far, for the release's record
GATE_LOGS=""

# Nothing uncommitted or untracked: the build is of a commit, so the record's commit id says what shipped.
gate_clean_tree() {
  local dirty
  dirty="$(git status --porcelain)" || return 1
  [ -z "$dirty" ] && return 0
  printf '%s\n' "$dirty"
  echo "uncommitted changes: commit or stash them first"
  return 1
}

gate_xtask_check() {
  (cd src-tauri && cargo run -q -p xtask -- check)
}

# The inventory and lock graph, regenerated from the code, are what's committed.
gate_inventory_current() {
  (cd src-tauri && cargo run -q -p xtask -- inventory) || return 1
  git diff --exit-code -- docs/dev/inventory.md docs/dev/lock-graph.dot && return 0
  echo "docs/dev/inventory.md is stale: commit the regenerated one (left in the tree)"
  return 1
}

# The Swift helpers build from nothing (a scratch folder of their own), not only from a warm build cache.
gate_swift_helpers() {
  local scratch failed=0 pkg
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/cue-release-swift.XXXX")" || return 1
  swiftc -O -o "$scratch/cue-listen" src-tauri/listen/main.swift || failed=1
  for pkg in speak voice; do
    swift build -c release --package-path "src-tauri/$pkg" --scratch-path "$scratch/$pkg" || failed=1
  done
  rm -rf "$scratch"
  return "$failed"
}

# One gate: `name`, then the command. Passes quietly into its log, or stops the release with the log's end.
run_gate() {
  local name="$1" start=$SECONDS log
  shift
  log="$GATE_LOGS/$(printf '%s' "$name" | tr ' ' '-').log"
  if "$@" >"$log" 2>&1; then
    say "passed  $name ($((SECONDS - start)) s)"
    GATES_PASSED="$GATES_PASSED${GATES_PASSED:+, }$name"
  else
    say "FAILED  $name: $log"
    tail -15 "$log" | sed 's/^/          /' >&2
    die "the $name gate failed: not releasing"
  fi
}

# Every gate, cheapest first, with their logs in $1. --skip-gates runs only the clean tree (its reason is
# recorded): a build is always of a commit, and a run that fails puts the version files back from it.
run_gates() {
  GATE_LOGS="$1"
  mkdir -p "$GATE_LOGS"
  run_gate "clean tree" gate_clean_tree
  if [ -n "$SKIP_GATES" ]; then
    say "the other gates skipped: $SKIP_GATES"
    return 0
  fi
  say "gates (logs in $GATE_LOGS)"
  run_gate "xtask check" gate_xtask_check
  run_gate "inventory current" gate_inventory_current
  run_gate "swift helpers" gate_swift_helpers
  run_gate "unit tests" tests/run.sh unit
  run_gate "ui tests" tests/run.sh ui
  run_gate "e2e tests" tests/run.sh e2e
  run_gate "soak $SOAK_MINUTES min" tests/run.sh soak "$SOAK_MINUTES"
}
