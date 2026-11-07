# Releasing Cue

`scripts/release.sh` builds, signs, notarizes and (when asked) publishes a release. It refuses to build
until every gate below passes. The script's parts are in `scripts/release/` (`preflight.sh`, `gates.sh`,
`build.sh`, `publish.sh`, `record.sh`, `channel.py`), and their tests are `tests/release/gates.test.sh`,
which `tests/run.sh unit` runs.

The usual way: a beta first, then a promote a day later.

```sh
scripts/release.sh 0.1.90 "What changed, for the update card." --beta --dry-run   # the gates only
scripts/release.sh 0.1.90 "What changed, for the update card." --beta             # build, publish to beta
# … use it for a day (Settings → General → Get beta builds) …
scripts/release.sh --promote 0.1.90                                                 # everyone gets it
```

`scripts/release-next.sh [--beta] [--dry-run] [--hotfix "<why>"] [--skip-gates "<why>"]` does the same with
the next patch version, notes from `.release-notes`, and commits and pushes the version bump. A dry run
commits nothing.

## Gates

Run in this order. The first one that fails stops the release, and its log is in
`dist/v<version>/gates/` (or `dist/dry-run-v<version>/gates/` for a dry run).

1. **clean tree:** `git status --porcelain` is empty, untracked files included. The record names the commit,
   so the build must be that commit.
2. **xtask check:** `cargo run -p xtask -- check` (lints, function and file sizes, lock order).
3. **inventory current:** `cargo run -p xtask -- inventory`, then `git diff --exit-code` on
   `docs/dev/inventory.md` and `docs/dev/lock-graph.dot`. A stale one is left regenerated in the tree to commit.
4. **swift helpers:** cue-listen (`swiftc`), and cue-speak and cue-voice (`swift build`), each built into a new
   scratch folder, so a half-purged build cache can't hide a broken build.
5. **unit, ui, e2e tests:** `tests/run.sh unit`, `ui`, `e2e`, one gate each.
6. **soak 10 min:** `tests/run.sh soak 10`. Until the soak harness lands, that prints "soak: not yet" and
   succeeds; the gate counts that as a failure.

Before the gates and before anything in `dist/` is removed, every build (dry or not, published or not)
checks that its version isn't tagged here or released on GitHub: a released version's `dist/v<version>` holds
the dSYM for its hang reports, so it's never built over. Then a run that ships (`--publish`, `--beta`, `--promote`) checks that its version comes after
the latest stable release (stable never goes back, and a beta older than stable would never be offered) and
the one-ship rule (below). A real run (not a dry run) also
checks the signing key, the update key and notarytool before the gates, so a missing key doesn't wait
out a soak.

`--skip-gates "<why>"` runs only the clean-tree gate. The reason is required and recorded.

The build is the commit plus the version written into `package.json`, `tauri.conf.json`, `Cargo.toml` and
`Cargo.lock`. `release.sh` never commits: `release-next.sh` commits the bump only once the release is
published. A run that stops before it finishes (a failed build, notarization or upload) puts those files
back as committed, so the next try passes the clean-tree gate. The clean tree is checked first, so this
never touches your own edits. A run that fails after GitHub already has the release is refused next time
(the version is out): finish it by hand with `gh`, or release the next version.

## Dry run

`--dry-run` runs the one-ship check (when publishing) and every gate, writes the record, prints it, and
stops. Nothing is built, the version isn't bumped, nothing goes to GitHub, and no keychain item is read.
An example (the timings left out):

```
  gates (logs in …/dist/dry-run-v0.1.90/gates)
  passed  clean tree (… s)
  passed  xtask check (… s)
  passed  inventory current (… s)
  passed  swift helpers (… s)
  passed  unit tests (… s)
  passed  ui tests (… s)
  passed  e2e tests (… s)
  passed  soak 10 min (… s)
  dry run: Cue 0.1.90 (beta) would be built now; nothing built, nothing published
    Cue 0.1.90 (beta), 2026-11-08 09:12 PST
    commit: …
    gates passed: clean tree, xtask check, inventory current, swift helpers, unit tests, ui tests, e2e tests, soak 10 min
    …
```

`--promote <version> --dry-run` makes every check a promote makes and changes nothing on GitHub.

## One ship a day

A run that ships (`--publish`, `--beta`, `--promote`) reads the releases on GitHub (`gh api …/releases`) and
refuses when a `v*` release already shipped today (your Mac's date): published today, stable or beta, or
promoted today. A promote's date is when its `latest.json` was uploaded, which only a stable publish or a
promote does. A build that isn't published isn't a ship. `--hotfix "<why>"` allows the second one; the
reason is recorded. A promote also waits a day on beta, sooner only with `--hotfix`.

## What gets recorded

`dist/v<version>/release-record.txt`: the version and channel, the date, the commit (plus the version bump), the gates passed and
where their logs are (or `gates: SKIPPED (all but the clean tree), <why>`), `hotfix: <why>` when given, Cargo.lock's sha256 and
`rustc --version`. The release's notes on GitHub end with "Released without its gates: <why>" or
"Hotfix: <why>" when either applies, so they stay visible after `dist/` is gone. A promote with `--hotfix`
adds "Promoted after N h on beta, a hotfix: <why>" to the release's notes.

## Beta and stable channels

Cue's updater asks one file: stable reads `releases/latest/download/latest.json` (the endpoint in
`src-tauri/tauri.conf.json`); with Settings → General → **Get beta builds** on (`updates.beta`, off by
default), it reads `latest-beta.json` beside it (`src-tauri/src/ipc/updates.rs`). Both files live on the
latest stable release, and each is a copy of some release's own `update.json` (its signed `Cue.app.tar.gz`
and that file's URL).

- `--publish`: a stable release with the dmg, the update, `update.json`, `latest.json`, and
  `latest-beta.json` (this release, unless a newer beta is out).
- `--beta`: a GitHub prerelease with the dmg, the update and `update.json` (GitHub's latest stays the stable
  one), and `latest-beta.json` on the latest stable release replaced to point at it.
- `--promote <version>`: refuses unless that version is a prerelease whose `update.json` offers it and is a
  day old (or `--hotfix`). Uploads its `update.json` as `latest.json` (and `latest-beta.json`, unless a newer
  beta is out) to that release, then makes it GitHub's latest. Same signed build, nothing rebuilt.

`latest-beta.json` is never older than `latest.json`, so beta users get every stable release too.

## Symbols (dSYMs)

Each build keeps `dist/v<version>/Cue.dSYM.zip` (both chips' symbols joined, checked to have the app's UUIDs)
and `dSYM-uuids.txt`. They aren't published. Keep them: they are what resolves a hang report from that
build (`atos -o Cue.dSYM/Contents/Resources/DWARF/cue -arch arm64 -l <load address> <addresses>`).

## Manual checks

The suites can't reach the real microphone, speaker or Notification Center. On the build about to ship
(the beta, before promoting), go through `docs/dev/manual-checks.md`: talk and hear the answer, dictate into a
text box, and a notification that comes and goes.

## Testing the script itself

`tests/release/gates.test.sh` runs `scripts/release.sh` in a throwaway git repo with stand-ins for the
suites, `cargo`, `swift`, `swiftc` and `gh`: every gate green, a failing suite, a dirty tree, an untracked
file, a stale inventory, a failing xtask check, no soak harness, a second stable or beta release the same
day, a hotfix, a build that isn't published, skipped gates, missing reasons, and promotes (too soon, a day
old, the wrong version). It never builds or publishes anything. Never test the script by running a real
release.
