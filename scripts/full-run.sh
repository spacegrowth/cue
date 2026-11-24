#!/bin/bash
# The full run: every test of Cue at one commit, in a clean copy kept for it, and one result file that says what
# failed and which commit broke it. Cards run only their own tests (tests/run.sh card); this runs everything once
# for what landed (docs/faster-checks-plan.md, docs/dev/test-speed.md). A plain script: whoever starts it (the Desk
# after a landing, on this machine or another machine, or you by hand) reads the result file.
#
#   scripts/full-run.sh --at <commit> [--since <last passed commit>] [--dir <work folder>] [--out <result file>] [--again]
#
# Run from inside a clone or checkout of the repository. It takes the whole machine for many minutes.
#
# <work folder> (CUE_FULL_RUN_DIR, or cue-full-run in your cache folder) is kept between runs:
#   tree/         the clean copy: a detached git worktree of the repository it was started from, moved to each
#                 commit. What git ignores stays (src-tauri/target, node_modules), so its build is warm.
#   logs/         one log per step of the latest run (find-<commit>-<step>.log: the search for the culprit)
#   result.json   the latest run's result (--out puts it elsewhere)
#   lock          one run at a time per work folder: a second call waits for the first, using no processor while
#                 it waits; a call waiting for an older commit than another waiting call isn't made (exit 2)
#   queue/        the calls waiting for their turn, one file each
# Asked again for a run already made (the result file holds a finished result for the same --at and --since), it
# runs nothing: it prints that file's path and exits as that run did. So a caller that lost track of a run (it quit,
# its connection dropped) asks for the same run again, waits behind the one still going, and gets its result.
# --again makes the run anyway.
# The checkout it was started from is never touched beyond having that worktree added to it.
#
# The steps, in order, each timed, each with its own log: build, code check, rust tests, node tests, window tests.
# A failed build stops the rest; any other failure doesn't, so one run reports everything that fails.
#
# With --since, when something failed: what failed is first run once more at <commit>. What passes that time fails
# only sometimes: it is listed in `flaky` and blamed on no commit. For what still fails, the first-parent commits of
# since..at are searched by halving, in the same copy, running only that (those Rust tests, those node test files,
# those specs, or that step); the first commit where it fails is the culprit. A node test file or spec that isn't in
# a commit doesn't fail there (so one added failing is blamed on the commit that added it), as a Rust test's name
# that matches no test doesn't. Failing at <since> itself: no culprit, it failed before this batch. A search that ends
# on a commit that changed only documents: no culprit either, what failed fails only sometimes. The copy is put
# back at <commit> afterwards.
#
# The result file, JSON:
#   at, since       the commits, in full (since: null when not given)
#   passed          true when every step passed
#   started, ended  milliseconds since 1970
#   steps           [{name, ok, secs, log}] for each step that ran
#   failed          names: Rust tests, node test files, spec files, or a step's name when it names nothing
#   culprit         {commit, subject}, or null
#   culprit_note    why there is no culprit, or what else to know about it
#   flaky           the names of `failed` that passed when run once more: they fail only sometimes
#   tail            the last 60 lines of the first failing step's log
# It never says where it ran. The last line this prints is the result file's path.
# Exit: 0 passed, 1 something failed, 2 the run couldn't be made (a bad commit, no copy): then there is no result file.
#
# CUE_FULL_RUN_STEPS names a file sourced after the steps below are defined: it may replace any of make_buildable,
# step_build, step_check, step_rust, step_node, step_window (tests/release/full_run.test.sh does, with stand-ins).
# Each runs in the copy's top folder, its output in the step's log; the last three take the names to run, none for all.
set -uo pipefail
unset TMUX TMUX_PANE

usage() { echo "usage: scripts/full-run.sh --at <commit> [--since <last passed commit>] [--dir <work folder>] [--out <result file>] [--again]" >&2; exit 2; }
die() { echo "full-run: $*" >&2; exit 2; }

AT="" SINCE="" DIR="${CUE_FULL_RUN_DIR:-}" OUT="" AGAIN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --at) AT="${2:-}"; shift ;;
    --since) SINCE="${2:-}"; shift ;;
    --dir) DIR="${2:-}"; shift ;;
    --out) OUT="${2:-}"; shift ;;
    --again) AGAIN=1 ;;
    *) usage ;;
  esac
  shift || usage
done
[ -n "$AT" ] || usage

REPO="$(git rev-parse --show-toplevel 2>/dev/null)" || die "run it from inside a checkout of the repository"
if [ -z "$DIR" ]; then
  if [ -d "$HOME/Library/Caches" ]; then DIR="$HOME/Library/Caches/cue-full-run"; else DIR="${XDG_CACHE_HOME:-$HOME/.cache}/cue-full-run"; fi
fi
mkdir -p "$DIR/logs" || die "couldn't make $DIR"
DIR="$(cd "$DIR" && pwd -P)"
case "$DIR/" in "$(cd "$REPO" && pwd -P)"/*) die "the work folder ($DIR) must be outside the repository" ;; esac
[ -n "$OUT" ] || OUT="$DIR/result.json"
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac
TREE="$DIR/tree"
LOGS="$DIR/logs"

AT_FULL="$(git -C "$REPO" rev-parse --verify -q "$AT^{commit}")" || die "no commit $AT here"
SINCE_FULL=""
if [ -n "$SINCE" ]; then
  SINCE_FULL="$(git -C "$REPO" rev-parse --verify -q "$SINCE^{commit}")" || die "no commit $SINCE here"
fi

# One run at a time per work folder. The turn is this open file: nothing a step starts keeps it (9>&- below).
# lockf where there is one, flock where there isn't (CUE_FULL_RUN_LOCKF names lockf's program: a test's way to
# have none). A call that has to wait asks again every few seconds (CUE_FULL_RUN_POLL) and sleeps in between: lockf
# waiting on a descriptor (`lockf 9`) spins a whole processor core for as long as it waits.
LOCKF="${CUE_FULL_RUN_LOCKF:-lockf}"
if command -v "$LOCKF" >/dev/null 2>&1; then
  lock_try() { "$LOCKF" -s -t 0 9; }
elif command -v flock >/dev/null 2>&1; then
  lock_try() { flock -n 9; }
else
  die "neither lockf nor flock is here: one run at a time can't be kept"
fi
# The calls waiting for the turn: queue/<pid> holds the commit each waits to run. A call waiting for an older commit
# than another waiting call gives up: that one's run covers it.
QUEUE="$DIR/queue"
mkdir -p "$QUEUE" || die "couldn't make $QUEUE"
trap 'rm -f "$QUEUE/$$"' EXIT
# The later commit of this one's line another live call waits for, if any.
newer_waiting() {
  local f pid c
  for f in "$QUEUE"/*; do
    [ -f "$f" ] || continue
    pid="${f##*/}"
    [ "$pid" != "$$" ] || continue
    if ! kill -0 "$pid" 2>/dev/null; then rm -f "$f"; continue; fi
    c="$(cat "$f" 2>/dev/null)"
    [ -n "$c" ] && [ "$c" != "$AT_FULL" ] || continue
    if git -C "$REPO" merge-base --is-ancestor "$AT_FULL" "$c" 2>/dev/null; then echo "$c"; return 0; fi
  done
  return 1
}
exec 9>>"$DIR/lock"
echo "$AT_FULL" >"$QUEUE/$$"
said_waiting=0
while :; do
  if newer="$(newer_waiting)"; then
    echo "full-run: not made: a run of a later commit ($(git -C "$REPO" rev-parse --short "$newer")) is waiting, and it covers this one" >&2
    exit 2
  fi
  lock_try && break
  if [ "$said_waiting" = 0 ]; then echo "full-run: another full run is using $DIR; waiting for its end…" >&2; said_waiting=1; fi
  sleep "${CUE_FULL_RUN_POLL:-5}"
done
rm -f "$QUEUE/$$"

# The run already made: the result file is written whole, so one that is there is finished; it starts with the
# commits and whether it passed (write_result).
if [ "$AGAIN" = 0 ] && [ -f "$OUT" ]; then
  made="{\"at\":\"$AT_FULL\",\"since\":null,\"passed\":"
  [ -z "$SINCE_FULL" ] || made="{\"at\":\"$AT_FULL\",\"since\":\"$SINCE_FULL\",\"passed\":"
  case "$(head -c 400 "$OUT" 2>/dev/null)" in
    "$made"true*) echo "full-run: already made for these commits (--again makes it anyway): passed" >&2; echo "$OUT"; exit 0 ;;
    "$made"false*) echo "full-run: already made for these commits (--again makes it anyway): FAILED" >&2; echo "$OUT"; exit 1 ;;
  esac
fi

now_ms() { echo "$(( $(date +%s) * 1000 ))"; }

# A string as a JSON string: control characters dropped, tabs as spaces, lines joined with \n.
json() {
  local s
  s=$(printf '%s' "$1" | LC_ALL=C tr -d '\000-\010\013-\037' | LC_ALL=C tr '\t' ' ' | LC_ALL=C sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | awk 'NR > 1 { printf "%s", "\\n" } { printf "%s", $0 }')
  printf '"%s"' "$s"
}

# ---- the steps (each replaceable: CUE_FULL_RUN_STEPS) ---------------------------------------------------------

# Started by an app, the PATH is short: the steps get the login shell's (node, cargo, npm).
login_path() {
  local login; login="$("${SHELL:-/bin/zsh}" -lc 'printf %s "$PATH"' 2>/dev/null </dev/null || true)"
  export PATH="${login:+$login:}$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
}

# The copy made buildable, as scripts/self-install.sh does for its own: node_modules (installed again only when
# package-lock.json changed since the last install here), and cue-host's program, which git ignores and the app's
# build expects in src-tauri/binaries: the starting checkout's when it has one, built otherwise.
make_buildable() {
  login_path
  if [ -f package-lock.json ] && { [ ! -d node_modules ] || ! cmp -s package-lock.json "$DIR/package-lock.installed"; }; then
    npm ci || return 1
    cp package-lock.json "$DIR/package-lock.installed"
  fi
  mkdir -p src-tauri/binaries
  local f have=0
  for f in "$REPO"/src-tauri/binaries/cue-host-*; do [ -e "$f" ] && cp -p "$f" src-tauri/binaries/; done
  for f in src-tauri/binaries/cue-host-*; do [ -e "$f" ] && have=1; done
  [ "$have" = 1 ] || scripts/build-host.sh
}

step_build() { cd src-tauri && cargo build; }
step_check() { cd src-tauri && cargo run -p xtask -- check; }
# Every test, or the ones named (in full, as cargo's "test … FAILED" lines name them). CUE_FULL_RUN: this is the
# run scripts/test-runner.sh leaves the full suite to. Never CUE_QUIET here: the tests set what they need.
step_rust() {
  cd src-tauri || return 1
  if [ $# -gt 0 ]; then CUE_FULL_RUN=1 cargo test --lib -- --exact "$@"; else CUE_FULL_RUN=1 cargo test --lib; fi
}
# Every node test file, or the ones named; each that fails is said on a line of its own, and the rest still run.
step_node() {
  local t bad=0 files=("$@")
  [ $# -gt 0 ] || files=(tests/*.test.js tests/ui/*.test.js)
  for t in "${files[@]}"; do
    [ -f "$t" ] || { echo "full-run: no $t at this commit"; continue; }
    node "$t" || { echo "node test $t ... FAILED"; bad=1; }
  done
  return $bad
}
# On a port free now, not the copy's own fixed one: a page server left running by a run that was stopped holds that
# one, and then every run's window tests failed before any started ("is already used").
free_port() { node -e 'const s = require("net").createServer().listen(0, "127.0.0.1", () => { console.log(s.address().port); s.close(); })'; }
step_window() { CUE_UI_PORT="${CUE_UI_PORT:-$(free_port)}" npx playwright test -c tests/ui/playwright.config.mjs "$@"; }

if [ -n "${CUE_FULL_RUN_STEPS:-}" ]; then
  [ -f "$CUE_FULL_RUN_STEPS" ] || die "no steps file $CUE_FULL_RUN_STEPS"
  # shellcheck disable=SC1090
  . "$CUE_FULL_RUN_STEPS"
fi

# ---- the copy --------------------------------------------------------------------------------------------------

# Whether tree/ is a checkout of its own, of this same repository: only then is it reused. A folder inside some
# other repository would otherwise have that one checked out and cleaned.
own_tree() {
  [ -d "$TREE" ] || return 1
  local top; top="$(git -C "$TREE" rev-parse --show-toplevel 2>/dev/null)" || return 1
  [ -n "$top" ] && [ "$(cd "$top" && pwd -P)" = "$(cd "$TREE" && pwd -P)" ] || return 1
  [ "$(git -C "$TREE" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" = "$(git -C "$REPO" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" ]
}

# The copy at commit `$1`, buildable. Only files git doesn't ignore are cleaned away, so its builds stay.
move_copy() {
  if own_tree; then
    git -C "$TREE" checkout -q --detach --force "$1" && git -C "$TREE" clean -fdq || return 1
  else
    rm -rf "$TREE"
    git -C "$REPO" worktree prune
    git -C "$REPO" worktree add -q --detach --force "$TREE" "$1" || return 1
  fi
  [ "$(git -C "$TREE" rev-parse HEAD 2>/dev/null)" = "$1" ] || return 1
  (cd "$TREE" && make_buildable) 9>&-
}

# ---- running ---------------------------------------------------------------------------------------------------

STEPS_JSON=""      # the result's steps, as they end
FIRST_FAILED_LOG=""
BUILD_OK=1
FAILED=()          # every name that failed, as the result lists them
F_RUST=() F_NODE=() F_SPEC=() F_STEPS=()   # the same, by how each is run again

# One step: `$1` its name, `$2` its log's name, then its function and arguments. Timed, recorded; its status returned.
run_step() {
  local name="$1" log="$LOGS/$2.log" t0=$SECONDS status
  shift 2
  echo "full-run: ${name}…" >&2
  (cd "$TREE" && "$@") >"$log" 2>&1 9>&- </dev/null
  status=$?
  local ok=true; [ "$status" = 0 ] || ok=false
  STEPS_JSON="$STEPS_JSON${STEPS_JSON:+,}{\"name\":$(json "$name"),\"ok\":$ok,\"secs\":$((SECONDS - t0)),\"log\":$(json "$log")}"
  if [ "$status" != 0 ]; then
    echo "full-run: $name FAILED (see $log)" >&2
    [ -n "$FIRST_FAILED_LOG" ] || FIRST_FAILED_LOG="$log"
  fi
  return $status
}

# What a failed step's log names, one a line: cargo's "test <name> ... FAILED", this script's "node test <file> ...
# FAILED", and Playwright's numbered failures ("  1) [cue] › desk-list.spec.mjs:12:3 › …"), each once.
names_rust() { LC_ALL=C sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$1" | LC_ALL=C sed 's/ - should panic$//' | sort -u; }
names_node() { LC_ALL=C sed -n 's/^node test \(.*\) \.\.\. FAILED$/\1/p' "$1" | sort -u; }
names_spec() { LC_ALL=C grep -E '^ +[0-9]+\) ' "$1" | LC_ALL=C grep -oE '[^ ]+\.spec\.mjs:[0-9]+:[0-9]+' | LC_ALL=C sed 's/:[0-9]*:[0-9]*$//' | sort -u; }

FLAKY=()           # what failed and then passed when run once more
write_result() {  # passed culprit-json note
  local failed="" flaky="" n tail=""
  for n in ${FAILED[@]+"${FAILED[@]}"}; do failed="$failed${failed:+,}$(json "$n")"; done
  for n in ${FLAKY[@]+"${FLAKY[@]}"}; do flaky="$flaky${flaky:+,}$(json "$n")"; done
  [ -z "$FIRST_FAILED_LOG" ] || tail="$(tail -n 60 "$FIRST_FAILED_LOG" 2>/dev/null)"
  local since=null; [ -z "$SINCE_FULL" ] || since="$(json "$SINCE_FULL")"
  mkdir -p "$(dirname "$OUT")"
  printf '{"at":%s,"since":%s,"passed":%s,"started":%s,"ended":%s,"steps":[%s],"failed":[%s],"culprit":%s,"culprit_note":%s,"flaky":[%s],"tail":%s}\n' \
    "$(json "$AT_FULL")" "$since" "$1" "$STARTED" "$(now_ms)" "$STEPS_JSON" "$failed" "$2" "$(json "$3")" "$flaky" "$(json "$tail")" >"$OUT.tmp" \
    && mv -f "$OUT.tmp" "$OUT"
}

STARTED="$(now_ms)"
rm -f "$OUT" "$LOGS"/*.log
move_copy "$AT_FULL" >"$LOGS/copy.log" 2>&1 || { echo "full-run: couldn't make the clean copy at $AT in $TREE:" >&2; tail -n 20 "$LOGS/copy.log" >&2; exit 2; }

if run_step "build" build step_build; then
  run_step "code check" check step_check || F_STEPS+=("code check")
  if ! run_step "rust tests" rust step_rust; then
    while IFS= read -r n; do [ -n "$n" ] && F_RUST+=("$n"); done < <(names_rust "$LOGS/rust.log")
    [ ${#F_RUST[@]} -gt 0 ] || F_STEPS+=("rust tests")
  fi
  if ! run_step "node tests" node step_node; then
    while IFS= read -r n; do [ -n "$n" ] && F_NODE+=("$n"); done < <(names_node "$LOGS/node.log")
    [ ${#F_NODE[@]} -gt 0 ] || F_STEPS+=("node tests")
  fi
  if ! run_step "window tests" window step_window; then
    while IFS= read -r n; do [ -n "$n" ] && F_SPEC+=("$n"); done < <(names_spec "$LOGS/window.log")
    [ ${#F_SPEC[@]} -gt 0 ] || F_STEPS+=("window tests")
  fi
else
  BUILD_OK=0
  F_STEPS+=("build")
fi
FAILED=(${F_STEPS[@]+"${F_STEPS[@]}"} ${F_RUST[@]+"${F_RUST[@]}"} ${F_NODE[@]+"${F_NODE[@]}"} ${F_SPEC[@]+"${F_SPEC[@]}"})

if [ ${#FAILED[@]} -eq 0 ]; then
  write_result true null "" || die "couldn't write $OUT"
  echo "full-run: passed" >&2
  echo "$OUT"
  exit 0
fi

# ---- the culprit -----------------------------------------------------------------------------------------------

# One step's function run in the copy: `$1` its log, then the function and what it's to run. Its status.
run_in_copy() {
  local log="$1"; shift
  (cd "$TREE" && "$@") >"$log" 2>&1 9>&- </dev/null
}

step_fn() {
  case "$1" in
    build) echo step_build ;; "code check") echo step_check ;; "rust tests") echo step_rust ;;
    "node tests") echo step_node ;; "window tests") echo step_window ;;
  esac
}

# Whether commit `$1` has file `$2`: by that path, or under any folder (a spec is named from its tests' folder).
in_commit() {
  git -C "$REPO" ls-tree -r --name-only "$1" | awk -v n="$2" '$0 == n || (length($0) > length(n) && substr($0, length($0) - length(n)) == "/" n) { f = 1 } END { exit !f }'
}

# Only what failed, run again in the copy at commit `$1`. 0: it fails there. 1: it passes. 2: the copy couldn't be
# put there. A node test file or a spec the commit doesn't have isn't run and doesn't fail there (run all the same,
# its runner would say there is no such test and fail); a Rust test's name that matches no test passes by itself.
fails_at() {
  local short log bad=0 s n node=() spec=()
  short="$(git -C "$REPO" rev-parse --short "$1")"
  log="$LOGS/find-$short"
  move_copy "$1" >"$log-copy.log" 2>&1 || return 2
  for s in ${F_STEPS[@]+"${F_STEPS[@]}"}; do
    run_in_copy "$log-${s%% *}.log" "$(step_fn "$s")" || bad=1
  done
  for n in ${F_NODE[@]+"${F_NODE[@]}"}; do if in_commit "$1" "$n"; then node+=("$n"); fi; done
  for n in ${F_SPEC[@]+"${F_SPEC[@]}"}; do if in_commit "$1" "$n"; then spec+=("$n"); fi; done
  if [ ${#F_RUST[@]} -gt 0 ]; then run_in_copy "$log-rust.log" step_rust "${F_RUST[@]}" || bad=1; fi
  if [ ${#node[@]} -gt 0 ]; then run_in_copy "$log-node.log" step_node "${node[@]}" || bad=1; fi
  if [ ${#spec[@]} -gt 0 ]; then run_in_copy "$log-window.log" step_window "${spec[@]}" || bad=1; fi
  [ "$bad" = 1 ]
}

# The names in STILL (what failed, of one kind) that still fail after being run once more with status `$1`: those
# its log `$2` names with `$3`, all of them when it failed and named none; the others are left in PASSED.
STILL=()
PASSED=()
still_failing() {
  local status="$1" log="$2" names="$3" n named="" keep=() gone=()
  [ "$status" = 0 ] || named="$("$names" "$log")"
  for n in ${STILL[@]+"${STILL[@]}"}; do
    if [ "$status" != 0 ] && { [ -z "$named" ] || grep -qxF -- "$n" <<<"$named"; }; then keep+=("$n"); else gone+=("$n"); fi
  done
  STILL=(${keep[@]+"${keep[@]}"})
  PASSED=(${gone[@]+"${gone[@]}"})
}

# What failed, run once more where it failed (the copy is at --at): what passes now fails only sometimes, and no
# commit is searched for it.
run_again() {
  local s status keep=()
  echo "full-run: running what failed once more…" >&2
  for s in ${F_STEPS[@]+"${F_STEPS[@]}"}; do
    if run_in_copy "$LOGS/again-${s%% *}.log" "$(step_fn "$s")"; then FLAKY+=("$s"); else keep+=("$s"); fi
  done
  F_STEPS=(${keep[@]+"${keep[@]}"})
  if [ ${#F_RUST[@]} -gt 0 ]; then
    run_in_copy "$LOGS/again-rust.log" step_rust "${F_RUST[@]}"; status=$?
    STILL=("${F_RUST[@]}"); still_failing "$status" "$LOGS/again-rust.log" names_rust; F_RUST=(${STILL[@]+"${STILL[@]}"}); FLAKY+=(${PASSED[@]+"${PASSED[@]}"})
  fi
  if [ ${#F_NODE[@]} -gt 0 ]; then
    run_in_copy "$LOGS/again-node.log" step_node "${F_NODE[@]}"; status=$?
    STILL=("${F_NODE[@]}"); still_failing "$status" "$LOGS/again-node.log" names_node; F_NODE=(${STILL[@]+"${STILL[@]}"}); FLAKY+=(${PASSED[@]+"${PASSED[@]}"})
  fi
  if [ ${#F_SPEC[@]} -gt 0 ]; then
    run_in_copy "$LOGS/again-window.log" step_window "${F_SPEC[@]}"; status=$?
    STILL=("${F_SPEC[@]}"); still_failing "$status" "$LOGS/again-window.log" names_spec; F_SPEC=(${STILL[@]+"${STILL[@]}"}); FLAKY+=(${PASSED[@]+"${PASSED[@]}"})
  fi
}

# What still fails, run at --since: what fails there too failed before this batch, goes to BEFORE and is searched
# for no further, so that one test broken for long doesn't hide the commit that broke another. 2: the copy couldn't
# be put there. A node test file or a spec --since doesn't have doesn't fail there (see fails_at).
BEFORE=()
drop_failing_before() {
  local log s n status keep=() have=() rest=()
  log="$LOGS/since"
  move_copy "$SINCE_FULL" >"$log-copy.log" 2>&1 || return 2
  for s in ${F_STEPS[@]+"${F_STEPS[@]}"}; do
    if run_in_copy "$log-${s%% *}.log" "$(step_fn "$s")"; then keep+=("$s"); else BEFORE+=("$s"); fi
  done
  F_STEPS=(${keep[@]+"${keep[@]}"})
  if [ ${#F_RUST[@]} -gt 0 ]; then
    run_in_copy "$log-rust.log" step_rust "${F_RUST[@]}"; status=$?
    STILL=("${F_RUST[@]}"); still_failing "$status" "$log-rust.log" names_rust
    BEFORE+=(${STILL[@]+"${STILL[@]}"}); F_RUST=(${PASSED[@]+"${PASSED[@]}"})
  fi
  for n in ${F_NODE[@]+"${F_NODE[@]}"}; do if in_commit "$SINCE_FULL" "$n"; then have+=("$n"); else rest+=("$n"); fi; done
  if [ ${#have[@]} -gt 0 ]; then
    run_in_copy "$log-node.log" step_node "${have[@]}"; status=$?
    STILL=("${have[@]}"); still_failing "$status" "$log-node.log" names_node
    BEFORE+=(${STILL[@]+"${STILL[@]}"}); F_NODE=(${rest[@]+"${rest[@]}"} ${PASSED[@]+"${PASSED[@]}"})
  fi
  have=(); rest=()
  for n in ${F_SPEC[@]+"${F_SPEC[@]}"}; do if in_commit "$SINCE_FULL" "$n"; then have+=("$n"); else rest+=("$n"); fi; done
  if [ ${#have[@]} -gt 0 ]; then
    run_in_copy "$log-window.log" step_window "${have[@]}"; status=$?
    STILL=("${have[@]}"); still_failing "$status" "$log-window.log" names_spec
    BEFORE+=(${STILL[@]+"${STILL[@]}"}); F_SPEC=(${rest[@]+"${rest[@]}"} ${PASSED[@]+"${PASSED[@]}"})
  fi
  return 0
}

# Whether commit `$1` changed only documents (Markdown, a licence) or nothing at all, against its first
# parent: no test or build reads those, so it can't have broken anything, and a search that ends on it was led there
# by a test that fails only sometimes.
only_documents() {
  local f
  while IFS= read -r f; do
    case "$f" in
      "") ;;
      *.md|*.markdown|LICENSE|*/LICENSE) ;;
      *) return 1 ;;
    esac
  done < <(git -C "$REPO" diff --no-renames --name-only "$1^1" "$1" 2>/dev/null || echo "?")
  return 0
}

CULPRIT=null
NOTE="no --since given: nothing to search"
find_culprit() {
  local c status subject commits=() lo hi mid left
  if [ "$SINCE_FULL" = "$AT_FULL" ]; then NOTE="--since is the commit that was run: no commits to search"; return; fi
  if ! git -C "$REPO" merge-base --is-ancestor "$SINCE_FULL" "$AT_FULL" 2>/dev/null; then
    NOTE="--since ($(git -C "$REPO" rev-parse --short "$SINCE_FULL")) isn't an earlier commit of this one's line: not searched"; return
  fi
  run_again
  left=$(( ${#F_STEPS[@]} + ${#F_RUST[@]} + ${#F_NODE[@]} + ${#F_SPEC[@]} ))
  if [ "$left" = 0 ]; then NOTE="culprit unknown: what failed passed when run once more, so it fails only sometimes"; return; fi
  echo "full-run: looking for the commit that broke it…" >&2
  drop_failing_before; status=$?
  if [ "$status" = 2 ]; then NOTE="couldn't put the copy at --since ($(git -C "$REPO" rev-parse --short "$SINCE_FULL")): not searched"; return; fi
  left=$(( ${#F_STEPS[@]} + ${#F_RUST[@]} + ${#F_NODE[@]} + ${#F_SPEC[@]} ))
  if [ "$left" = 0 ]; then NOTE="culprit unknown: it already fails at --since ($(git -C "$REPO" rev-parse --short "$SINCE_FULL")), before this batch"; return; fi
  while IFS= read -r c; do [ -n "$c" ] && commits+=("$c"); done < <(git -C "$REPO" rev-list --first-parent --reverse "$SINCE_FULL..$AT_FULL")
  # By halving: it passes before commits[lo+1] (at --since) and fails at commits[hi] (the last one, --at).
  lo=-1; hi=$(( ${#commits[@]} - 1 ))
  if [ "$hi" -lt 0 ]; then NOTE="no commits between --since and the commit that was run: not searched"; return; fi
  while [ $((hi - lo)) -gt 1 ]; do
    mid=$(( (lo + hi) / 2 ))
    fails_at "${commits[$mid]}"; status=$?
    if [ "$status" = 2 ]; then NOTE="couldn't put the copy at $(git -C "$REPO" rev-parse --short "${commits[$mid]}"): the search stopped there"; return; fi
    if [ "$status" = 0 ]; then hi=$mid; else lo=$mid; fi
  done
  c="${commits[$hi]}"
  subject="$(git -C "$REPO" log -1 --format=%s "$c")"
  if only_documents "$c"; then
    NOTE="culprit unknown: the search ended on $(git -C "$REPO" rev-parse --short "$c") ($subject), which changed only documents, so what failed fails only sometimes"
    return
  fi
  CULPRIT="{\"commit\":$(json "$c"),\"subject\":$(json "$subject")}"
  NOTE="the first commit since --since where what failed fails"
  [ "$left" -le 1 ] || NOTE="$NOTE (several things failed: a later commit may have broken some of them)"
  [ ${#FLAKY[@]} -eq 0 ] || NOTE="$NOTE; some of it passed when run once more and is blamed on no commit"
  [ ${#BEFORE[@]} -eq 0 ] || NOTE="$NOTE; already failing before this batch and blamed on no commit: ${BEFORE[*]}"
}
if [ -n "$SINCE_FULL" ]; then
  find_culprit
  # Back where the run was made, for whoever reads the copy next.
  git -C "$TREE" checkout -q --detach --force "$AT_FULL" 2>/dev/null && git -C "$TREE" clean -fdq 2>/dev/null
fi
[ "$BUILD_OK" = 1 ] || NOTE="$NOTE; the build failed, so no test ran"

write_result false "$CULPRIT" "$NOTE" || die "couldn't write $OUT"
echo "full-run: FAILED: ${FAILED[*]}" >&2
echo "$OUT"
exit 1
