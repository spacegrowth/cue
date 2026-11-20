#!/bin/bash
# Refuse to publish words that must never be in this public repo: names, hosts, private projects.
# The words live outside the repo, one per line, in <git common dir>/info/private-words (never committed,
# shared by every worktree of this clone). Checks the code at HEAD and every commit not yet on the remote:
# the lines each one adds and its message, since a word removed later is still in the pushed history.
#
#   scripts/check-private-words.sh            HEAD's code + the commits since origin/main
#   scripts/check-private-words.sh <base>     the commits since <base> instead
#
# Use it as a pre-push hook too: ln -s ../../scripts/check-private-words.sh .git/hooks/pre-push
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1
words="${PRIVATE_WORDS:-$(git rev-parse --git-common-dir)/info/private-words}"
if [ ! -s "$words" ]; then
  echo "no private words listed in $words: nothing to check"
  exit 0
fi
base="${1:-origin/main}"
case "$base" in -*|origin) base=origin/main ;; esac   # run as a pre-push hook, $1 is the remote's name
# No such base (a clone with no remote yet): every commit is still to be published.
if git rev-parse -q --verify "$base^{commit}" >/dev/null; then range="$base..HEAD"; else range=HEAD; base="the start"; fi
pattern="$(grep -v '^[[:space:]]*\(#\|$\)' "$words")"
found=0

# The code as it is now.
if hits="$(git grep -n -i -I -F -f <(printf '%s\n' "$pattern") HEAD -- . 2>/dev/null)" && [ -n "$hits" ]; then
  echo "in the code at HEAD:"; printf '%s\n' "$hits" | cut -c1-200; found=1
fi
# Each commit not yet pushed: its message and the lines it adds.
for c in $(git rev-list "$range"); do
  hit="$( { git log -1 --format=%B "$c"; git show --format= --no-color "$c" | grep '^+' | grep -v '^+++'; } \
    | grep -i -F -f <(printf '%s\n' "$pattern") | cut -c1-200)"
  if [ -n "$hit" ]; then
    echo "in commit $(git log -1 --format='%h %s' "$c"):"; printf '%s\n' "$hit" | sed 's/^/  /'; found=1
  fi
done
if [ "$found" = 1 ]; then
  echo "private words found: take them out (rewrite the unpushed commits) before publishing"
  exit 1
fi
echo "no private words in HEAD or the $(git rev-list --count "$range") commits since $base"
