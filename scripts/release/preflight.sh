# The checks before the gates (sourced by scripts/release.sh), for a run that ships (--publish, --beta or
# --promote): the version comes after the stable one, and it's the only ship today.

# A run that puts a version in front of anyone.
ships() {
  [ "$CHANNEL" != none ] || [ "$PROMOTE" = 1 ]
}

# The latest stable release's tag.
latest_stable_tag() {
  gh release view --repo "$REPO" --json tagName --jq .tagName
}

# Stable never goes back, and a beta older than stable would never be offered: the version must come after it.
check_newer_than_stable() {
  local stable
  stable="$(latest_stable_tag)" || die "couldn't ask GitHub for the latest stable release"
  channel_py is-newer "$VERSION" "$stable" || die "$VERSION isn't newer than the stable release ($stable)"
}

# One ship a day: a second ship today (a stable or beta release published, or a beta promoted) is refused unless
# it's a --hotfix, whose reason is recorded. A promote counts from when its latest.json was uploaded (channel.py
# ship_times). Batching is the point: seven ships in a day was the symptom.

check_ship_day() {
  local listed today
  listed="$(gh api "repos/$REPO/releases?per_page=30")" || die "couldn't list the releases on GitHub (gh api …/releases)"
  today="$(printf '%s' "$listed" | channel_py shipped-today)"
  [ -n "$today" ] || return 0
  [ -n "$HOTFIX" ] || die "already released today ($today): one ship a day. Fold this into tomorrow's, or --hotfix \"why\" if it can't wait"
  say "a second release today (after $today): hotfix, $HOTFIX"
}

run_preflight() {
  ships || return 0   # a build that isn't published isn't a ship
  check_newer_than_stable
  check_ship_day
}
