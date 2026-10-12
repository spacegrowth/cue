# Extensions

An extension adds a tab to the open session in Cue, beside Chat, with a page of its own. Cue comes with
one, [Diff](../extensions/diff), which shows what a session changed.

## Install one

1. In Cue, open **Settings → Extensions** and click **Open the folder**.
2. Copy the extension's folder in (the folder with `manifest.json` in it; unzip it first if it came as
   a zip). Its folder name is its name: letters, digits, `-` and `_`.
3. Click **Look again**. It's listed, switched off, with what it asks to do.
4. Switch it on.

Nothing in an extension runs until you switch it on. Switching it on approves what its manifest asks
for at that moment; Cue keeps that in its own settings, not in the extension's folder. If an update
asks for more, it's switched off again, with the new asks listed, until you switch it back on.
Settings also lists the last commands each one ran (all of them are in `extensions.log` in Cue's
folder).

## Make one

An extension is a folder:

```
my-ext/
  manifest.json
  index.html        and whatever .js, .css and images it loads
```

### manifest.json

```json
{
  "title": "My extension",
  "tab": "Mine",
  "version": "0.1.0",
  "entry": "index.html",
  "capabilities": ["sessions.read", "cmd.run"],
  "programs": ["git"]
}
```

| Field | |
|---|---|
| `title` | Its name in Settings. |
| `tab` | The tab's label in a session. |
| `version` | Shown in Settings. |
| `entry` | The page the tab opens. Default `index.html`. |
| `capabilities` | What it asks to do (below). Leave it out to ask for nothing. |
| `programs` | The programs it may run, by name, e.g. `["git"]`. Nothing else will run. |

| Capability | Lets it |
|---|---|
| `sessions.read` | Know the open session: its name, folder, machine, state, and where its turns began. |
| `cmd.run` | Run its `programs` on this Mac. |
| `machines.run` | Run its `programs` on the machine a session is on, over Cue's own SSH connection. |

### The page

The tab shows your page in a sandboxed frame. It can load its own scripts, styles and images, but it
can't reach Cue's window or the network, or fetch anything (its own files included: put data in a
script); it talks to Cue only through Cue's helper. Load the helper, and Cue's stylesheet if you want its look
(its colors follow Cue's light and dark):

```html
<link rel="stylesheet" href="cueext://localhost/_cue/cue.css">
<script src="cueext://localhost/_cue/cue.js"></script>
<script src="main.js" defer></script>
```

Scripts must be files of your own (no inline `<script>`). Your page loads when Cue starts and stays
loaded while Cue runs, so its tab can show a count before it's opened.

### cue.js

Every call returns a promise, and fails with a message saying why when the manifest didn't ask for it.

| Call | |
|---|---|
| `cue.ready` | Resolves once Cue has answered (with its theme, already applied). |
| `cue.session()` | The open session, or `null` (needs `sessions.read`). |
| `cue.on("session", f)` | `f(session)` whenever the open session changes, or anything about it does (needs `sessions.read`). |
| `cue.on("theme", f)` | `f(theme)` when Cue's look changes. |
| `cue.run(program, args, { cwd, machine })` | Runs a program from your `programs`, without a shell, for at most 20 seconds. Resolves to `{ code, stdout, stderr, line, ms }`. With `machine` (a session's `machine`), it runs there (needs `machines.run`); without it, on this Mac (needs `cmd.run`). |
| `cue.badge(sessionId, text)` | Short text on your tab for that session, e.g. `"4 +98 −9"`. `""` clears it. |
| `cue.toast(text)` | A short message in Cue. |
| `cue.log(text)` | A line in `extensions.log`. |

A session looks like this:

```json
{
  "id": "…", "name": "api-refactor", "folder": "/Users/me/code/api", "machine": "",
  "state": "waiting", "agent": "claude",
  "turn_from": { "root": "/Users/me/code/api", "commit": "3f9a1c2…", "untracked": ["notes.md"] },
  "session_from": { "root": "/Users/me/code/api", "commit": "8b04e7d…", "untracked": [] }
}
```

`machine` is empty for a session on this Mac. `turn_from` is where the latest turn began and
`session_from` where its first turn began: a git snapshot of the folder then (diff against `commit`;
`untracked` were the untracked files), or `null` outside a git repository.

### Trying it out

Put the folder in Cue's extensions folder, click **Look again**, switch it on, and open a session.
After you change its files, switch it off and on again in Settings to reload its page. [Diff](../extensions/diff) is a complete example.
