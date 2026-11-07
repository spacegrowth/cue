# One ship a day (sourced by scripts/release.sh): a second release (stable or beta) published today is refused
# unless it's a --hotfix, whose reason is recorded. Batching is the point: seven ships in a day was the symptom.

check_ship_day() {
  [ "$CHANNEL" != none ] || return 0   # a build that isn't published isn't a ship
  local listed today
  listed="$(gh release list --repo "$REPO" --limit 30 --json tagName,publishedAt)" || die "couldn't list the releases on GitHub (gh release list)"
  today="$(printf '%s' "$listed" | channel_py shipped-today)"
  [ -n "$today" ] || return 0
  [ -n "$HOTFIX" ] || die "already released today ($today): one ship a day. Fold this into tomorrow's, or --hotfix \"why\" if it can't wait"
  say "a second release today (after $today): hotfix, $HOTFIX"
}
