#!/bin/bash
# scripts/full-run.sh against stand-ins: a throwaway repository of a few commits and a steps file
# (CUE_FULL_RUN_STEPS) whose steps pass or fail by what the commit holds, each recording what it was asked. Nothing
# is built, no test of Cue's runs.
#   tests/release/full_run.test.sh      (tests/run.sh unit runs it)
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cue-full-run-test.XXXX")"
WORK="$(cd "$WORK" && pwd -P)"
trap 'rm -rf "$WORK"' EXIT
FAILED=0
CASE_FAILED=0
CASE=""
OUT=""
STATUS=0
export CALLS="$WORK/calls"

# ---- the stand-in steps ----------------------------------------------------------------------------------------
# A commit says what is broken in it by the files it holds: broken-build, broken-check, broken-rust (the names of the
# Rust tests that fail, one a line), broken-node (node test files), broken-window (spec files), slow (seconds the
# build takes), flaky-rust (Rust tests that fail the first time they are run in a case and pass after). Asked for a
# node test file or a spec by name, the node and window steps fail whenever the commit names it broken, whether the
# file is there or not: as the real runners fail on a file that isn't. Each step appends "<step> <commit's subject> [names…]" to $CALLS.
cat >"$WORK/steps.sh" <<'STEPS'
called() { echo "$1 $(git log -1 --format=%s)${2:+ $2}" >>"$CALLS"; }
make_buildable() { called buildable; }
step_build() {
  called build-start
  [ ! -f slow ] || sleep "$(cat slow)"
  called build
  [ ! -f broken-build ] || { echo "error[E0308]: mismatched types"; return 1; }
}
step_check() { called check; [ ! -f broken-check ] || { echo "check: a lock taken inside a longer expression"; return 1; }; }
step_rust() {
  called rust "$*"
  echo "running 3 tests"
  echo "test desk::lands::tests::it_passes ... ok"
  local n bad=1
  if [ -f flaky-rust ] && [ ! -f "$CALLS.flaked" ]; then
    touch "$CALLS.flaked"
    while IFS= read -r n; do echo "test $n ... FAILED"; done <flaky-rust
    return 1
  fi
  [ -f broken-rust ] || { echo "test result: ok."; return 0; }
  while IFS= read -r n; do echo "test $n ... FAILED"; done <broken-rust
  echo "test result: FAILED."
  return $bad
}
step_node() {
  called node "$*"
  [ -f broken-node ] || return 0
  local n
  while IFS= read -r n; do echo "node test $n ... FAILED"; done <broken-node
  return 1
}
step_window() {
  called window "$*"
  [ -f broken-window ] || { echo "  12 passed"; return 0; }
  local n i=0
  while IFS= read -r n; do i=$((i + 1)); echo "  $i) [cue] › $n:12:3 › a row stays put ────"; echo "    Error: expected 1, got 2"; done <broken-window
  echo "  $i failed"
  return 1
}
STEPS

# ---- running a case --------------------------------------------------------------------------------------------
# A case starts from a fresh repository (one commit, "base", with a node test file and a spec the cases break) and a
# fresh work folder.
commit() {  # subject, then what to do in the repository first
  local subject="$1"; shift
  (cd "$REPO" && eval "${1:-:}" && git add -A && git -c user.email=t@t -c user.name=t commit -q --allow-empty -m "$subject")
  git -C "$REPO" rev-parse HEAD
}

start() {
  CASE="$1"; CASE_FAILED=0
  rm -rf "$WORK/repo" "$WORK/run"
  REPO="$WORK/repo"; RUN="$WORK/run"
  mkdir -p "$REPO"
  (cd "$REPO" && git init -q && echo "cue" >README.md && mkdir -p tests/ui && touch tests/box.test.js tests/ui/desk-list.spec.mjs)
  BASE="$(commit base)"
  : >"$CALLS"; rm -f "$CALLS.flaked" "$CALLS.lock"
}

full_run() {
  OUT="$(cd "$REPO" && CUE_FULL_RUN_STEPS="$WORK/steps.sh" "$ROOT/scripts/full-run.sh" --dir "$RUN" "$@" 2>&1)"
  STATUS=$?
  RESULT="$RUN/result.json"
}

fail() { printf 'FAIL %s: %s\n%s\n%s\n' "$CASE" "$1" "$OUT" "$(cat "$RESULT" 2>/dev/null)" | sed '2,$s/^/    /'; FAILED=1; CASE_FAILED=1; }
exits() { [ "$STATUS" = "$1" ] || fail "exit $STATUS, not $1"; }
says() { grep -qF -- "$1" <<<"$OUT" || fail "doesn't say: $1"; }
# The result file's field `$1` (a path for node: steps.0.name), as JSON.
field() { node -e 'let v = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")); for (const k of process.argv[2].split(".")) v = v == null ? undefined : v[k]; console.log(JSON.stringify(v))' "$RESULT" "$1" 2>&1; }
is() { local got; got="$(field "$1")"; [ "$got" = "$2" ] || fail "$1 is $got, not $2"; }
called() { grep -qxF -- "$1" "$CALLS" || fail "never ran: $1"; }
not_called() { if grep -qF -- "$1" "$CALLS"; then fail "ran: $1"; fi; }
done_case() { if [ "$CASE_FAILED" = 0 ]; then printf 'ok   %s\n' "$CASE"; fi; }

start "a passing run says passed, with every step, and its last line is the result file"
full_run --at "$BASE"
exits 0; is passed true; is at "\"$BASE\""; is since null; is failed "[]"; is culprit null; is tail '""'
is steps.0.name '"build"'; is steps.1.name '"code check"'; is steps.2.name '"rust tests"'; is steps.3.name '"node tests"'; is steps.4.name '"window tests"'
is steps.4.ok true; is steps.2.log "\"$RUN/logs/rust.log\""
[ "$(tail -n 1 <<<"$OUT")" = "$RESULT" ] || fail "the last line isn't the result file"
[ "$(field started)" -le "$(field ended)" ] 2>/dev/null || fail "started isn't before ended"
[ "$(git -C "$RUN/tree" rev-parse HEAD)" = "$BASE" ] || fail "the copy isn't at the commit"
[ -z "$(git -C "$REPO" status --porcelain)" ] || fail "the starting checkout was changed"
if grep -qiE "$(hostname -s)|$(id -un)" <(sed "s|$RUN||g; s|$WORK||g" "$RESULT"); then fail "the result says where it ran"; fi
done_case

start "failing tests give passed=false and their names; the later steps still run"
AT="$(commit "break two" 'printf "desk::lands::tests::it_lands\ntesting::tests::desk_land::done_lands\n" >broken-rust; echo desk-list.spec.mjs >broken-window; echo tests/box.test.js >broken-node')"
full_run --at "$AT"
exits 1; is passed false; is culprit null
is failed '["desk::lands::tests::it_lands","testing::tests::desk_land::done_lands","tests/box.test.js","desk-list.spec.mjs"]'
is steps.0.ok true; is steps.2.ok false; is steps.3.ok false; is steps.4.ok false
grep -qF "it_lands ... FAILED" <<<"$(field tail)" || fail "the tail isn't the first failing step's log"
called "window break two"
[ "$(tail -n 1 <<<"$OUT")" = "$RESULT" ] || fail "the last line isn't the result file"
done_case

start "a failed build stops the rest and is named by its step; a failed code check doesn't stop the tests"
AT="$(commit "no build" 'touch broken-build')"
full_run --at "$AT"
exits 1; is failed '["build"]'; is steps.length 1; not_called "rust"; not_called "check"
grep -qF "mismatched types" <<<"$(field tail)" || fail "the tail isn't the build's log"
AT="$(commit "builds, check finds" 'git rm -q broken-build; touch broken-check')"
: >"$CALLS"
full_run --at "$AT"
exits 1; is failed '["code check"]'; is steps.length 5; called "rust builds, check finds"
done_case

start "with --since, the culprit is the first commit where what failed fails, found by running only that"
C1="$(commit "one: fine" 'echo a >a')"
C2="$(commit "two: breaks landing" 'echo desk::lands::tests::it_lands >broken-rust')"
C3="$(commit "three: fine too" 'echo c >c')"
full_run --at "$C3" --since "$BASE"
exits 1; is passed false; is since "\"$BASE\""; is failed '["desk::lands::tests::it_lands"]'
is culprit.commit "\"$C2\""; is culprit.subject '"two: breaks landing"'
called "rust base desk::lands::tests::it_lands"; called "rust one: fine desk::lands::tests::it_lands"; called "rust two: breaks landing desk::lands::tests::it_lands"
not_called "node one: fine"; not_called "window two"; not_called "build two"
# What failed is run once more where it failed before any commit is searched; that one is never searched again.
[ "$(grep -cxF "rust three: fine too desk::lands::tests::it_lands" "$CALLS")" = 1 ] || fail "what failed wasn't run once more at --at, once"
is flaky "[]"
[ "$(git -C "$RUN/tree" rev-parse HEAD)" = "$C3" ] || fail "the copy wasn't put back at --at"
done_case

start "the culprit of a failed step is found by that step alone, and of a spec by that spec"
C1="$(commit "one: fine" 'echo a >a')"
C2="$(commit "two: check finds" 'touch broken-check')"
C3="$(commit "three: a spec breaks" 'echo desk-list.spec.mjs >broken-window')"
full_run --at "$C2" --since "$BASE"
exits 1; is culprit.commit "\"$C2\""; called "check one: fine"; not_called "rust one: fine"
(cd "$REPO" && git rm -q broken-check && git -c user.email=t@t -c user.name=t commit -qm "four: check fixed")
C4="$(git -C "$REPO" rev-parse HEAD)"
: >"$CALLS"
full_run --at "$C4" --since "$C2"
exits 1; is failed '["desk-list.spec.mjs"]'; is culprit.commit "\"$C3\""; is culprit.subject '"three: a spec breaks"'
called "window two: check finds desk-list.spec.mjs"; not_called "check two"
done_case

start "failing already at --since: no culprit, and the note says it failed before this batch"
C1="$(commit "one: broken from the start" 'echo tests/box.test.js >broken-node')"
C2="$(commit "two" 'echo b >b')"
C3="$(commit "three" 'echo c >c')"
full_run --at "$C3" --since "$C1"
exits 1; is failed '["tests/box.test.js"]'; is culprit null
grep -qF "culprit unknown: it already fails at --since" <<<"$(field culprit_note)" || fail "the note doesn't say it failed before"
grep -qF "before this batch" <<<"$(field culprit_note)" || fail "the note doesn't say before this batch"
not_called "node two tests/box.test.js"
[ "$(git -C "$RUN/tree" rev-parse HEAD)" = "$C3" ] || fail "the copy wasn't put back at --at"
done_case

start "one test failing before the batch doesn't hide the commit that broke another"
C1="$(commit "one: a node test broken from the start" 'echo tests/box.test.js >broken-node')"
C2="$(commit "two" 'echo b >b')"
C3="$(commit "three: a Rust test breaks" 'echo desk::lands::tests::new_one >broken-rust')"
C4="$(commit "four" 'echo d >d')"
full_run --at "$C4" --since "$C1"
exits 1; is failed '["desk::lands::tests::new_one","tests/box.test.js"]'; is flaky "[]"
is culprit.commit "\"$C3\""; is culprit.subject '"three: a Rust test breaks"'
grep -qF "already failing before this batch and blamed on no commit: tests/box.test.js" <<<"$(field culprit_note)" || fail "the note doesn't name what failed before"
called "node one: a node test broken from the start tests/box.test.js"; not_called "node two tests/box.test.js"; not_called "node three"
called "rust two desk::lands::tests::new_one"
done_case

start "what passes when run once more fails only sometimes: listed as flaky, blamed on no commit"
C1="$(commit "one: fine" 'echo a >a')"
C2="$(commit "two: one sometimes, one always" 'echo a::sometimes >flaky-rust; echo tests/box.test.js >broken-node')"
full_run --at "$C2" --since "$BASE"
exits 1; is failed '["a::sometimes","tests/box.test.js"]'; is flaky '["a::sometimes"]'
is culprit.commit "\"$C2\""; not_called "rust base"; not_called "rust one: fine"; called "node one: fine tests/box.test.js"
grep -qF "passed when run once more" <<<"$(field culprit_note)" || fail "the note doesn't say some of it fails only sometimes"
C3="$(commit "three: only the one that fails sometimes" 'git rm -q broken-node')"
rm -f "$CALLS.flaked"; : >"$CALLS"
full_run --at "$C3" --since "$C1"
exits 1; is passed false; is failed '["a::sometimes"]'; is flaky '["a::sometimes"]'; is culprit null
grep -qF "fails only sometimes" <<<"$(field culprit_note)" || fail "the note doesn't say it fails only sometimes"
not_called "rust one: fine"; not_called "buildable two"
done_case

start "a spec or node test file added failing is blamed on the commit that added it: it fails nowhere it isn't"
C1="$(commit "one: would fail if asked" 'echo new.spec.mjs >broken-window; echo tests/new.test.js >broken-node')"
C2="$(commit "two: adds them" 'touch tests/new.test.js tests/ui/new.spec.mjs')"
C3="$(commit "three" 'echo c >c')"
C4="$(commit "four" 'echo d >d')"
full_run --at "$C4" --since "$C1"
exits 1; is failed '["tests/new.test.js","new.spec.mjs"]'; is culprit.commit "\"$C2\""; is culprit.subject '"two: adds them"'
not_called "window one: would fail if asked new.spec.mjs"; not_called "node one: would fail if asked tests/new.test.js"
called "window two: adds them new.spec.mjs"; called "node two: adds them tests/new.test.js"
done_case

start "the culprit is searched by halving, not commit by commit"
for n in 1 2 3 4 5; do commit "c$n" "echo $n >f" >/dev/null; done
C6="$(commit "c6" 'touch broken-check')"
commit "c7" 'echo 7 >f' >/dev/null
C8="$(commit "c8" 'echo 8 >f')"
: >"$CALLS"
full_run --at "$C8" --since "$BASE"
exits 1; is failed '["code check"]'; is culprit.commit "\"$C6\""
# The run, once more at --at, at --since, then three halvings of eight commits (c4, c6, c5).
[ "$(grep -c "^check " "$CALLS" | tr -d ' ')" = 6 ] || fail "the check ran $(grep -c "^check " "$CALLS") times, not 6: $(grep "^check " "$CALLS" | tr '\n' ',')"
called "check c4"; called "check c5"; called "check c6"; not_called "check c1"; not_called "check c2"; not_called "check c7"
done_case

start "asked again for a run already made, it runs nothing and answers as that run did; --again makes it"
C1="$(commit "one: breaks" 'echo tests/box.test.js >broken-node')"
full_run --at "$C1" --since "$BASE"
exits 1; is culprit.commit "\"$C1\""
WAS="$(cat "$RESULT")"; : >"$CALLS"
full_run --at "$C1" --since "$BASE"
exits 1; says "already made for these commits"; not_called "build"
[ "$(tail -n 1 <<<"$OUT")" = "$RESULT" ] || fail "the last line isn't the result file"
[ "$(cat "$RESULT")" = "$WAS" ] || fail "the result was written again"
full_run --at "$C1"
exits 1; is since null; called "build one: breaks"
: >"$CALLS"
full_run --at "$BASE"
exits 0; called "build base"
: >"$CALLS"
full_run --at "$BASE"
exits 0; says "already made for these commits"; not_called "build"
full_run --at "$BASE" --again
exits 0; called "build base"
done_case

start "the same run asked for twice at once is made once: the second waits and gets the first's result"
SLOW="$(commit "slow" 'echo 2 >slow')"
(cd "$REPO" && CUE_FULL_RUN_STEPS="$WORK/steps.sh" "$ROOT/scripts/full-run.sh" --dir "$RUN" --at "$SLOW" >"$WORK/first.out" 2>&1) &
FIRST=$!
for _ in $(seq 1 100); do grep -q "^build-start slow" "$CALLS" && break; sleep 0.1; done
full_run --at "$SLOW"
exits 0; says "another full run is using"; says "already made for these commits"; is passed true; is at "\"$SLOW\""
wait "$FIRST" || fail "the first call failed: $(cat "$WORK/first.out")"
[ "$(grep -c "^build slow" "$CALLS" | tr -d ' ')" = 1 ] || fail "the run was made $(grep -c "^build slow" "$CALLS") times"
done_case

start "with no lockf, flock keeps the one run at a time"
mkdir -p "$WORK/bin"
printf '#!/bin/bash\necho "flock $*" >>"$CALLS.lock"\n' >"$WORK/bin/flock"; chmod +x "$WORK/bin/flock"
CUE_FULL_RUN_LOCKF=no-such-lockf PATH="$WORK/bin:$PATH" full_run --at "$BASE"
exits 0; is passed true
[ "$(cat "$CALLS.lock" 2>/dev/null)" = "flock -n 9" ] || fail "flock wasn't asked for the turn: $(cat "$CALLS.lock" 2>/dev/null)"
done_case

start "a bad commit, or none, is no run: exit 2 and no result file"
full_run --at no-such-commit
exits 2; says "no commit no-such-commit here"; [ ! -e "$RESULT" ] || fail "a result file was written"
full_run --at "$BASE" --since no-such-commit
exits 2; says "no commit no-such-commit here"
full_run
exits 2; says "usage: scripts/full-run.sh --at"
not_called "build"
done_case

start "the copy is kept between runs, with what git ignores in it; --out says where the result goes"
C1="$(commit "one" 'echo target >.gitignore')"
full_run --at "$BASE"
exits 0
mkdir -p "$RUN/tree/target"; echo warm >"$RUN/tree/target/built"; echo stray >"$RUN/tree/stray.txt"
full_run --at "$C1" --out "$WORK/elsewhere/r.json"
exits 0
[ -f "$RUN/tree/target/built" ] || fail "the copy's ignored build folder was removed"
[ ! -e "$RUN/tree/stray.txt" ] || fail "a stray file was left in the copy"
[ "$(git -C "$RUN/tree" rev-parse HEAD)" = "$C1" ] || fail "the copy wasn't moved to the new commit"
[ "$(tail -n 1 <<<"$OUT")" = "$WORK/elsewhere/r.json" ] && [ -f "$WORK/elsewhere/r.json" ] || fail "--out wasn't where the result went"
[ "$(git -C "$REPO" worktree list | wc -l | tr -d ' ')" = 2 ] || fail "more than the one copy was added"
done_case

start "two calls at once run one after the other"
SLOW="$(commit "slow" 'echo 2 >slow')"
(cd "$REPO" && CUE_FULL_RUN_STEPS="$WORK/steps.sh" "$ROOT/scripts/full-run.sh" --dir "$RUN" --at "$SLOW" --out "$WORK/first.json" >"$WORK/first.out" 2>&1) &
FIRST=$!
for _ in $(seq 1 100); do grep -q "^build-start slow" "$CALLS" && break; sleep 0.1; done
full_run --at "$BASE" --out "$WORK/second.json"
exits 0; says "another full run is using"
wait "$FIRST" || fail "the first call failed: $(cat "$WORK/first.out")"
[ "$(grep -E "^(build-start|build|window) " "$CALLS" | tr '\n' ',')" = "build-start slow,build slow,window slow,build-start base,build base,window base," ] || fail "the two runs' steps were mixed: $(tr '\n' ',' <"$CALLS")"
RESULT="$WORK/first.json"; is passed true; is at "\"$SLOW\""
RESULT="$WORK/second.json"; is passed true; is at "\"$BASE\""
done_case

if [ "$FAILED" = 0 ]; then echo "full run: all cases passed"; else echo "full run: FAILED"; exit 1; fi
