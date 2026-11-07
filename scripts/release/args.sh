# What a release run was asked to do (sourced by scripts/release.sh). Sets VERSION, NOTES, CHANNEL (none: build
# only, stable: --publish, beta: --beta), PROMOTE, DRY_RUN, and the reasons HOTFIX and SKIP_GATES ("" when not given).

usage() {
  cat >&2 <<'USAGE'
usage: scripts/release.sh <version> "<notes>" [--publish | --beta] [--dry-run] [--hotfix "<why>"] [--skip-gates "<why>"]
       scripts/release.sh --promote <version> [--dry-run] [--hotfix "<why>"]
  --publish     a stable release: everyone's update (latest.json), and beta's too
  --beta        a beta release: only Cue's with Settings → Get beta builds on (latest-beta.json)
  --promote     a beta already published becomes stable: the same signed build, nothing rebuilt
  --dry-run     the gates (and the checks before publishing), then stop before building
  --hotfix      a second release today, or a promote before a day on beta: why it can't wait (recorded)
  --skip-gates  no gates: why (recorded in the release's notes on GitHub and its record in dist/)
USAGE
  exit 2
}

# The reason after an option that needs one: refused when it's missing or looks like another option.
reason_for() {  # $1 = the option, $2 = what followed it
  case "${2:-}" in ""|--*) die "$1 needs a reason: $1 \"why\"" ;; esac
  printf '%s' "$2"
}

parse_args() {
  VERSION=""; NOTES=""; CHANNEL="none"; PROMOTE=0; DRY_RUN=0; HOTFIX=""; SKIP_GATES=""
  local given=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --publish|--beta)
        [ "$CHANNEL" = none ] || die "--publish or --beta, not both"
        if [ "$1" = --publish ]; then CHANNEL=stable; else CHANNEL=beta; fi ;;
      --promote) PROMOTE=1; VERSION="${2:-}"; shift ;;
      --dry-run) DRY_RUN=1 ;;
      --hotfix) HOTFIX="$(reason_for "$1" "${2:-}")"; shift ;;
      --skip-gates) SKIP_GATES="$(reason_for "$1" "${2:-}")"; shift ;;
      -h|--help) usage ;;
      --*) die "unknown option $1 (scripts/release.sh --help)" ;;
      *)
        given=$((given + 1))
        case "$given" in 1) VERSION="$1" ;; 2) NOTES="$1" ;; *) usage ;; esac ;;
    esac
    shift
  done
  [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
  if [ "$PROMOTE" = 1 ]; then
    [ "$given" = 0 ] && [ "$CHANNEL" = none ] || die "--promote takes only a version (its notes are the beta's)"
    [ -z "$SKIP_GATES" ] || die "--promote has no gates to skip (the beta passed them when it was built)"
  else
    [ -n "$NOTES" ] || die "say what changed (it shows in Cue's update card)"
  fi
}
