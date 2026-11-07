#!/bin/bash
# scripts/release.sh's gates and checks, against stand-ins: a throwaway repo holding a copy of the release
# scripts, a tests/run.sh that passes or fails as a case says, and cargo, swift, swiftc and gh on PATH that
# only record what they were asked. Nothing is built, nothing reaches GitHub, nothing touches your Cue.
#   tests/release/gates.test.sh      (tests/run.sh unit runs it)
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cue-release-test.XXXX")"
trap 'rm -rf "$WORK"' EXIT
FAILED=0       # any case failed
CASE_FAILED=0  # this case failed
CASE=""

# ---- the stand-ins ----------------------------------------------------------------------------------------
# Each stand-in appends its arguments to $CALLS; what it answers is set per case through STUB_* variables.
make_stubs() {
  mkdir -p "$WORK/bin"
  cat >"$WORK/bin/cargo" <<'STUB'
#!/bin/bash
echo "cargo $*" >>"$CALLS"
case "$*" in
  *"xtask -- inventory"*) [ -n "${STUB_INVENTORY:-}" ] && printf '%s\n' "$STUB_INVENTORY" >../docs/dev/inventory.md; exit 0 ;;
  *"xtask -- check"*) [ -z "${STUB_XTASK_FAILS:-}" ] ;;
esac
STUB
  cat >"$WORK/bin/gh" <<'STUB'
#!/bin/bash
echo "gh $*" >>"$CALLS"
now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
case "$1 $2" in
  "release list")
    if [ -n "${STUB_SHIPPED_TODAY:-}" ]; then echo "[{\"tagName\":\"$STUB_SHIPPED_TODAY\",\"publishedAt\":\"$now\"}]"
    else echo '[{"tagName":"v0.1.80","publishedAt":"2026-01-02T03:04:05Z"}]'; fi ;;
  "release view")
    case "$*" in *isPrerelease*) echo true ;; *) echo v0.1.86 ;; esac ;;
  "release download")
    dir="$(printf '%s\n' "$@" | sed -n '/^--dir$/{n;p;}')"
    case "$*" in
      *update.json*) printf '{"version":"%s","notes":"n","pub_date":"%s","platforms":{}}' "${STUB_BETA_VERSION:-0.2.0}" "${STUB_BETA_PUBLISHED:-$now}" >"$dir/update.json" ;;
      *) exit 1 ;;
    esac ;;
  *) exit 0 ;;
esac
STUB
  printf '#!/bin/bash\necho "$(basename "$0") $*" >>"$CALLS"\n' >"$WORK/bin/swift"
  cp "$WORK/bin/swift" "$WORK/bin/swiftc"
  chmod +x "$WORK/bin/"*
}

# A repo with the release scripts as they are now, the inventory committed, and a tests/run.sh whose suites pass
# unless $STUB_FAILS names one; its soak says "not yet" while STUB_SOAK_NOT_YET is set, as tests/run.sh does now.
make_repo() {
  local repo="$WORK/repo"
  rm -rf "$repo"; mkdir -p "$repo/scripts" "$repo/tests" "$repo/docs/dev" "$repo/src-tauri"
  cp -R "$ROOT/scripts/release.sh" "$ROOT/scripts/release" "$repo/scripts/"
  echo "inventory" >"$repo/docs/dev/inventory.md"
  echo "lock graph" >"$repo/docs/dev/lock-graph.dot"
  echo "# a lock file" >"$repo/src-tauri/Cargo.lock"
  printf 'dist/\n' >"$repo/.gitignore"
  cat >"$repo/tests/run.sh" <<'STUB'
#!/bin/bash
echo "tests/run.sh $*" >>"$CALLS"
if [ "$1" = soak ] && [ -n "${STUB_SOAK_NOT_YET:-}" ]; then echo "soak: not yet (the soak harness lands as src-tauri/src/testing/soak.rs)"; exit 0; fi
[ "$1" != "${STUB_FAILS:-}" ] || { echo "1 failed"; exit 1; }
echo "$1: ok"
STUB
  chmod +x "$repo/tests/run.sh"
  (cd "$repo" && git init -q && git add -A && git -c user.email=t@t -c user.name=t commit -qm init)
  echo "$repo"
}

# ---- running a case ---------------------------------------------------------------------------------------
# A case starts from a fresh repo and no stand-in answers; `release` runs the script there, keeping its output.
start() {
  CASE="$1"; CASE_FAILED=0
  unset STUB_FAILS STUB_SOAK_NOT_YET STUB_INVENTORY STUB_XTASK_FAILS STUB_SHIPPED_TODAY STUB_BETA_VERSION STUB_BETA_PUBLISHED
  REPO="$(make_repo)"
  export CALLS="$WORK/calls"; : >"$CALLS"
}

release() {
  OUT="$(cd "$REPO" && PATH="$WORK/bin:$PATH" scripts/release.sh "$@" 2>&1)"
  STATUS=$?
}

fail() { printf 'FAIL %s: %s\n%s\n' "$CASE" "$1" "$OUT" | sed '3,$s/^/    /'; FAILED=1; CASE_FAILED=1; }
refused() { [ "$STATUS" != 0 ] || fail "should have been refused"; }
passed() { [ "$STATUS" = 0 ] || fail "should have passed"; }
says() { grep -qF -- "$1" <<<"$OUT" || fail "doesn't say: $1"; }
called() { grep -qF -- "$1" "$CALLS" || fail "never ran: $1"; }
not_called() { if grep -qF -- "$1" "$CALLS"; then fail "ran: $1"; fi; }
done_case() { if [ "$CASE_FAILED" = 0 ]; then printf 'ok   %s\n' "$CASE"; fi; }

make_stubs

start "a dry run with every gate green stops before building"
release 0.2.0 "Notes." --beta --dry-run
passed; says "dry run: Cue 0.2.0 (beta) would be built now"; says "gates passed: clean tree, xtask check, inventory current, swift helpers, unit tests, ui tests, e2e tests, soak 10 min"
called "tests/run.sh unit"; called "tests/run.sh ui"; called "tests/run.sh e2e"; called "tests/run.sh soak 10"; called "swift build -c release"
not_called "security"; not_called "gh release create"
[ -z "$(cd "$REPO" && git status --porcelain)" ] || fail "the dry run changed the tree (a version bumped?)"
done_case

start "a failing suite stops the release at that gate"
export STUB_FAILS=ui
release 0.2.0 "Notes." --publish --dry-run
refused; says "the ui tests gate failed: not releasing"; not_called "tests/run.sh e2e"
done_case

start "a dirty tree is refused before anything runs"
echo "edit" >>"$REPO/docs/dev/inventory.md"
release 0.2.0 "Notes." --dry-run
refused; says "the clean tree gate failed"; not_called "tests/run.sh"
done_case

start "an untracked file counts as dirty"
echo "new" >"$REPO/scratch.txt"
release 0.2.0 "Notes." --dry-run
refused; says "the clean tree gate failed"
done_case

start "a stale inventory is refused"
export STUB_INVENTORY="regenerated, different"
release 0.2.0 "Notes." --dry-run
refused; says "the inventory current gate failed"; not_called "tests/run.sh"
done_case

start "a failing xtask check is refused"
export STUB_XTASK_FAILS=1
release 0.2.0 "Notes." --dry-run
refused; says "the xtask check gate failed"
done_case

start "no soak harness yet is no soak: refused"
export STUB_SOAK_NOT_YET=1
release 0.2.0 "Notes." --dry-run
refused; says "the soak 10 min gate failed"
done_case

start "a second release the same day is refused"
export STUB_SHIPPED_TODAY=v0.1.99
release 0.2.0 "Notes." --publish --dry-run
refused; says "already released today (v0.1.99)"; not_called "tests/run.sh"
done_case

start "a second beta the same day is refused too"
export STUB_SHIPPED_TODAY=v0.1.99
release 0.2.0 "Notes." --beta --dry-run
refused; says "one ship a day"
done_case

start "a hotfix may be the second release of the day, and says why in its record"
export STUB_SHIPPED_TODAY=v0.1.99
release 0.2.0 "Notes." --publish --dry-run --hotfix "the update card crashes on launch"
passed; says "a second release today (after v0.1.99): hotfix, the update card crashes on launch"
says "hotfix: the update card crashes on launch"
done_case

start "a build that isn't published isn't a ship"
export STUB_SHIPPED_TODAY=v0.1.99
release 0.2.0 "Notes." --dry-run
passed; not_called "gh release list"
done_case

start "skipping the gates runs none and records why"
release 0.2.0 "Notes." --beta --dry-run --skip-gates "Apple's notary is down; tried the suites by hand"
passed; not_called "tests/run.sh"; not_called "cargo"
says "gates: SKIPPED, Apple's notary is down; tried the suites by hand"
done_case

start "a reason is required"
release 0.2.0 "Notes." --dry-run --skip-gates
refused; says "--skip-gates needs a reason"
release 0.2.0 "Notes." --publish --hotfix --dry-run
refused; says "--hotfix needs a reason"
done_case

start "--publish and --beta together are refused"
release 0.2.0 "Notes." --publish --beta --dry-run
refused; says "not both"
done_case

start "promoting a beta under a day old is refused"
release --promote 0.2.0 --dry-run
refused; says "has been on beta 0 h: give it a day"
done_case

start "promoting a day-old beta copies its own update file, nothing rebuilt"
export STUB_BETA_PUBLISHED=2026-01-01T00:00:00Z
release --promote 0.2.0 --dry-run
passed; says "dry run: v0.2.0"; says "would become stable, the same build"
called "gh release download v0.2.0 --repo spacegrowth/cue --pattern update.json"; not_called "gh release upload"; not_called "cargo"
done_case

start "a promote whose update file is another version's is refused"
export STUB_BETA_PUBLISHED=2026-01-01T00:00:00Z STUB_BETA_VERSION=0.1.9
release --promote 0.2.0 --dry-run
refused; says "offers another version"
done_case

if [ "$FAILED" = 0 ]; then echo "release gates: all cases passed"; else echo "release gates: FAILED"; exit 1; fi
