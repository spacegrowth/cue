"""The update files scripts/release.sh writes and reads (docs/dev/releasing.md).

update.json is one build's signed update, kept on its own release; latest.json (stable) and latest-beta.json
(beta) are the files Cue's updater asks for, each a copy of some release's update.json.

  channel.py entry <version> <notes> <signature> <url> <out>   write a build's update.json
  channel.py version <file>                                     the version a file offers
  channel.py newer <a> <b>                                      print the newer of two files (b may be missing)
  channel.py is-newer <version> <version>                       succeeds when the first comes after the second
  channel.py age-hours <file>                                   whole hours since the file's build was published
  channel.py shipped-today                                      from `gh release list --json tagName,publishedAt`
                                                                on stdin: the release tags published today, here
"""
import datetime
import json
import os
import re
import sys


def entry(version, notes, signature, url, out):
    both = {"signature": signature, "url": url}
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    update = {"version": version, "notes": notes, "pub_date": now, "platforms": {"darwin-aarch64": both, "darwin-x86_64": both}}
    with open(out, "w") as f:
        json.dump(update, f, indent=2)


def read(path):
    with open(path) as f:
        return json.load(f)


def version_key(v):
    return tuple(int(n) for n in v.lstrip("v").split("."))


def is_newer(a, b):
    sys.exit(0 if version_key(a) > version_key(b) else 1)


def newer(a, b):
    pick = a
    if os.path.exists(b) and version_key(read(b)["version"]) > version_key(read(a)["version"]):
        pick = b
    with open(pick) as f:
        sys.stdout.write(f.read())


def published(stamp):
    return datetime.datetime.strptime(stamp, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc)


def age_hours(path):
    age = datetime.datetime.now(datetime.timezone.utc) - published(read(path)["pub_date"])
    print(int(age.total_seconds() // 3600))


def shipped_today():
    today = datetime.datetime.now().astimezone().date()
    tags = [r["tagName"] for r in json.load(sys.stdin)
            if re.match(r"^v[0-9]", r.get("tagName", "")) and r.get("publishedAt")
            and published(r["publishedAt"]).astimezone().date() == today]
    print(", ".join(tags))


def main(args):
    commands = {
        "entry": (5, entry),
        "version": (1, lambda p: print(read(p)["version"])),
        "newer": (2, newer),
        "is-newer": (2, is_newer),
        "age-hours": (1, age_hours),
        "shipped-today": (0, shipped_today),
    }
    if not args or args[0] not in commands or len(args) - 1 != commands[args[0]][0]:
        sys.exit(__doc__)
    commands[args[0]][1](*args[1:])


if __name__ == "__main__":
    main(sys.argv[1:])
