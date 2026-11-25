#!/bin/bash
# scripts/test-runner.sh's one full test run at a time: a run waiting for another's end takes its turn once that one
# ends, and uses next to no processor while it waits (lockf waiting on the turn's descriptor used to spin a whole core).
# A throwaway repository and a stand-in test program; no test of Cue's runs.
#   tests/release/test_runner_lock.test.sh      (tests/run.sh unit runs it)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cue-test-runner-test.XXXX")"
WORK="$(cd "$WORK" && pwd -P)"
HOLDER=""
trap '[ -z "$HOLDER" ] || kill "$HOLDER" 2>/dev/null; rm -rf "$WORK"' EXIT
command -v lockf >/dev/null 2>&1 || { echo "test runner lock: no lockf here, nothing to test"; exit 0; }

git -C "$WORK" init -q repo
mkdir -p "$WORK/repo/target/debug/deps"
# The stand-in test program, named as the full suite's is: it says it ran.
printf '#!/bin/bash\necho ran\n' >"$WORK/repo/target/debug/deps/cue_lib-0000"
chmod +x "$WORK/repo/target/debug/deps/cue_lib-0000"
LOCK="$(git -C "$WORK/repo" rev-parse --path-format=absolute --git-common-dir)/cue-full-test-run.lock"

# Another full run holds the turn for 4 seconds.
( exec 9>>"$LOCK"; lockf -s -t 0 9 || exit 1; touch "$WORK/held"; sleep 4 ) &
HOLDER=$!
for _ in $(seq 1 50); do [ -e "$WORK/held" ] && break; sleep 0.1; done

TIMEFORMAT='%U %S %R'
{ time OUT="$(cd "$WORK/repo" && CUE_TEST_LOCK_POLL=1 "$ROOT/scripts/test-runner.sh" "$WORK/repo/target/debug/deps/cue_lib-0000" 2>&1)"; } 2>"$WORK/cpu"
STATUS=$?
read -r U S R <"$WORK/cpu"
FAILED=0
fail() { printf 'FAIL %s\n    %s\n' "$1" "$OUT"; FAILED=1; }
[ "$STATUS" = 0 ] || fail "exit $STATUS"
grep -qx "ran" <<<"$OUT" || fail "the test program never ran"
grep -qF "waiting for its end" <<<"$OUT" || fail "it didn't wait for the other run"
awk -v u="$U" -v s="$S" -v r="$R" 'BEGIN { exit !(r >= 2.5 && u + s < 0.5) }' || fail "waiting $R s took $U s user + $S s system of the processor"
wait "$HOLDER"; HOLDER=""
if [ "$FAILED" = 0 ]; then echo "test runner lock: all cases passed"; else echo "test runner lock: FAILED"; exit 1; fi
