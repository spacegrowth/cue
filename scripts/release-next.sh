#!/bin/bash
# The next release in one go: the next patch version (0.1.45 → 0.1.46), its notes from .release-notes at the
# repo root (git-ignored; write them first, a few plain sentences for Cue's update card), then
# scripts/release.sh … --publish, and the version committed and pushed as "Cue <version>: <notes>".
# The notes file is emptied once it's published. Stops before building when the notes are missing.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
NOTES="$(cat .release-notes 2>/dev/null || true)"
NOTES="$(printf '%s' "$NOTES" | tr '\n' ' ' | sed 's/  */ /g; s/^ //; s/ $//')"
[ -n "$NOTES" ] || { echo "release-next: write the notes in $ROOT/.release-notes first"; exit 1; }
# After the last published one (its tag), so a run that failed half way tries the same version again.
git fetch -q --tags
LAST="$(git tag --list 'v[0-9]*' --sort=-v:refname | head -1)"
[ -n "$LAST" ] || { echo "release-next: no release tag (v0.1.0…) to follow"; exit 1; }
VERSION="$(python3 -c 'import sys; a, b, c = sys.argv[1].lstrip("v").split("."); print(f"{a}.{b}.{int(c) + 1}")' "$LAST")"
echo "  releasing $VERSION: $NOTES"
scripts/release.sh "$VERSION" "$NOTES" --publish
git commit -q -m "Cue $VERSION: $NOTES" -- package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git pull -q --rebase --autostash
git push -q
: > .release-notes
echo "  committed and pushed Cue $VERSION"
