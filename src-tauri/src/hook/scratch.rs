//! A session's own scratch folder (#720): a shell command that only writes inside it runs without a permission
//! prompt, so work left running overnight never waits on you to make a folder, copy, unpack or build there.
//!
//! The scratch folder is the one Claude Code gives each session: `<tmp>/claude-<uid>/<project>/<session id>/scratchpad`
//! (`/tmp` or `/private/tmp`, or CLAUDE_CODE_TMPDIR). The agent's PreToolUse hook (hook.rs `guard`) says yes to such
//! a call itself; anything this can't read for sure is left to the agent's own flow (its prompt), as before.
//!
//! What counts, each part of the command read alone (parts joined by `&&`, `;`, a new line or `|`):
//! - `mkdir` of folders in it; `cp` into it; `mv` within it; `cd` into it;
//! - `tar` unpacking into it (`-C` or a `cd` there) or packing to a file in it; `git archive` (to a file in it or
//!   piped on);
//! - a build run there (cargo, npm/pnpm/yarn, make, go, swift);
//! - commands that only read (ls, cat, grep, find without its writing options, …).
//!
//! Output redirected anywhere but there (or /dev/null) is out, and so is anything this doesn't read word for word:
//! `$` (a variable or `$(…)`), backquotes, `~`, `..`, `||`, `&` (run in the background), a here-document. Deleting
//! is never in the list, so deletes, pushes and releases still go to Ask me before and their own prompt.

/// Whether `command`, run in folder `cwd` by session `sid`, only writes inside that session's scratch folder.
pub(crate) fn only_writes_in_scratch(command: &str, cwd: &str, sid: &str) -> bool {
    let roots = tmp_roots();
    let ours = |p: &str| in_scratch_dir(p, sid, &roots);
    if sid.is_empty() || sid.contains('/') {
        return false;
    }
    let Some(tokens) = tokens(command) else { return false };
    let start_in = ours(cwd);
    let mut here = start_in;
    // The cwd as the statement (an `&&` chain) began: after `;` the cwd is only sure to be the scratch folder if
    // it was then too (a `cd` that failed leaves it where it was).
    let mut chain_start = start_in;
    let mut seg: Vec<Tok> = vec![];
    let mut in_pipe = false;
    let mut checked_any = false;
    let mut iter = tokens.into_iter().peekable();
    loop {
        let next = iter.next();
        let ends = matches!(next, None | Some(Tok::Op(_)));
        if !ends {
            seg.push(next.unwrap());
            continue;
        }
        let op = match &next {
            Some(Tok::Op(o)) => o.as_str(),
            _ => "",
        };
        let piped = in_pipe || op == "|";
        if !seg.is_empty() {
            let Some(cd) = segment(&seg, here, piped, &ours) else { return false };
            if let Some(to) = cd {
                here = to;
            }
            checked_any = true;
        } else if !op.is_empty() && op != "\n" && op != ";" {
            return false; // `&& x` with nothing before it
        }
        seg.clear();
        in_pipe = op == "|";
        match op {
            "&&" | "|" => {}
            ";" | "\n" | "" => {
                here = here && chain_start;
                chain_start = here;
            }
            _ => return false,
        }
        if next.is_none() {
            break;
        }
    }
    checked_any
}

/// Where Claude Code's per-user temp folders can be: CLAUDE_CODE_TMPDIR's, else /tmp's (and /private/tmp, the same
/// folder on a Mac).
fn tmp_roots() -> Vec<String> {
    // SAFETY: getuid has no preconditions and can't fail.
    let uid = unsafe { libc::getuid() };
    let mut out = vec![];
    if let Ok(t) = std::env::var("CLAUDE_CODE_TMPDIR") {
        let t = t.trim_end_matches('/');
        if t.starts_with('/') && !t.is_empty() {
            out.push(format!("{t}/claude-{uid}"));
        }
    }
    out.push(format!("/tmp/claude-{uid}"));
    out.push(format!("/private/tmp/claude-{uid}"));
    out
}

/// Whether absolute `path` is session `sid`'s scratch folder or in it: `<root>/<project>/<sid>/scratchpad[/…]`.
fn in_scratch_dir(path: &str, sid: &str, roots: &[String]) -> bool {
    if !path.starts_with('/') || has_parent_step(path) {
        return false;
    }
    roots.iter().any(|r| {
        let Some(rest) = path.strip_prefix(r.as_str()).and_then(|x| x.strip_prefix('/')) else { return false };
        let mut parts = rest.split('/');
        let (Some(project), Some(s), Some(pad)) = (parts.next(), parts.next(), parts.next()) else { return false };
        !project.is_empty() && !project.starts_with('.') && s == sid && pad == "scratchpad"
    })
}

fn has_parent_step(path: &str) -> bool {
    path.split('/').any(|c| c == "..")
}

/// Whether word `p` names a place in the scratch folder: an absolute path there, or a relative one while the cwd
/// is there.
fn place_ok(p: &str, here: bool, ours: &dyn Fn(&str) -> bool) -> bool {
    if p.is_empty() || p.starts_with('~') || p.starts_with('-') || has_parent_step(p) {
        return false;
    }
    if p.starts_with('/') { ours(p) } else { here }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    /// `>` or `>>` (out) or `<` (in); the next word is its file.
    Redir(&'static str),
    /// `&&`, `||`, `;`, `|`, `&`, or a new line.
    Op(String),
}

/// `command` as words, redirections and operators, quotes taken off; None for anything not read for sure.
fn tokens(command: &str) -> Option<Vec<Tok>> {
    let mut lx = Lexer { c: command.chars().collect(), i: 0, out: vec![], word: String::new(), in_word: false, bare_digits: true };
    while lx.i < lx.c.len() {
        lx.step()?;
    }
    lx.flush();
    Some(lx.out)
}

/// Reads a command a character at a time.
struct Lexer {
    c: Vec<char>,
    i: usize,
    out: Vec<Tok>,
    word: String,
    in_word: bool,
    /// Whether the word so far is all unquoted digits (a file descriptor before `>`).
    bare_digits: bool,
}

impl Lexer {
    fn at(&self, k: usize) -> Option<char> {
        self.c.get(k).copied()
    }

    fn flush(&mut self) {
        if self.in_word {
            self.out.push(Tok::Word(std::mem::take(&mut self.word)));
            self.in_word = false;
        }
        self.bare_digits = true;
    }

    /// Text added to the word that can't be a file descriptor.
    fn add(&mut self, s: impl IntoIterator<Item = char>) {
        self.word.extend(s);
        self.in_word = true;
        self.bare_digits = false;
    }

    /// The character at `i` read, and `i` moved past it (and whatever it takes with it). None: not read for sure.
    fn step(&mut self) -> Option<()> {
        let ch = self.c[self.i];
        match ch {
            '$' | '`' => return None,
            '\'' => {
                let end = self.c[self.i + 1..].iter().position(|&x| x == '\'')? + self.i + 1;
                let text: Vec<char> = self.c[self.i + 1..end].to_vec();
                self.add(text);
                self.i = end + 1;
            }
            '"' => self.double_quoted()?,
            '\\' => {
                let x = self.at(self.i + 1)?;
                if x != '\n' {
                    self.add([x]);
                }
                self.i += 2;
            }
            ' ' | '\t' => {
                self.flush();
                self.i += 1;
            }
            '\n' | ';' => {
                self.flush();
                self.out.push(Tok::Op(ch.to_string()));
                self.i += 1;
            }
            '#' if !self.in_word => {
                // A comment, to the line's end.
                while self.i < self.c.len() && self.c[self.i] != '\n' {
                    self.i += 1;
                }
            }
            // Subshells, groups, brace lists (`{.,.}.` makes `..`), and a glob or history at a word's start: not
            // read here.
            '(' | ')' | '{' | '}' => return None,
            '*' | '?' | '[' | '!' if !self.in_word => return None,
            '&' | '|' => self.operator(ch),
            '<' | '>' => self.redirect(ch)?,
            _ => {
                if !ch.is_ascii_digit() {
                    self.bare_digits = false;
                }
                self.word.push(ch);
                self.in_word = true;
                self.i += 1;
            }
        }
        Some(())
    }

    fn double_quoted(&mut self) -> Option<()> {
        let mut j = self.i + 1;
        let mut text = String::new();
        loop {
            match self.at(j)? {
                '"' => break,
                '$' | '`' => return None,
                '\\' => {
                    text.push(self.at(j + 1)?);
                    j += 2;
                }
                x => {
                    text.push(x);
                    j += 1;
                }
            }
        }
        self.add(text.chars());
        self.i = j + 1;
        Some(())
    }

    /// `&`, `&&`, `|`, `||`, or `&>` (both streams to a file).
    fn operator(&mut self, ch: char) {
        self.flush();
        if ch == '&' && self.at(self.i + 1) == Some('>') {
            self.i += 2;
            if self.at(self.i) == Some('>') {
                self.i += 1;
            }
            self.out.push(Tok::Redir(">"));
        } else if self.at(self.i + 1) == Some(ch) {
            self.out.push(Tok::Op(format!("{ch}{ch}")));
            self.i += 2;
        } else {
            self.out.push(Tok::Op(ch.to_string()));
            self.i += 1;
        }
    }

    /// `<`, `>`, `>>`, `>|`, with a file descriptor before it (`2>`), or one stream to another (`2>&1`).
    fn redirect(&mut self, ch: char) -> Option<()> {
        if self.in_word && self.bare_digits {
            self.word.clear();
            self.in_word = false;
        }
        self.flush();
        if matches!((ch, self.at(self.i + 1)), ('<', Some('<' | '(')) | ('>', Some('('))) {
            return None; // a here-document, or a process read or written as a file
        }
        let mut j = self.i + 1;
        let mut kind: &'static str = if ch == '<' { "<" } else { ">" };
        if ch == '>' && self.at(j) == Some('>') {
            kind = ">>";
            j += 1;
        }
        if ch == '>' && self.at(j) == Some('|') {
            j += 1;
        }
        if self.at(j) == Some('&') {
            j += 1;
            let start = j;
            while self.at(j).is_some_and(|d| d.is_ascii_digit() || d == '-') {
                j += 1;
            }
            if j == start {
                return None;
            }
        } else {
            self.out.push(Tok::Redir(kind));
        }
        self.i = j;
        Some(())
    }
}

/// Commands that only read (their output, when redirected, is checked like any other).
const READERS: &[&str] = &["ls", "pwd", "echo", "printf", "true", "cat", "head", "tail", "wc", "grep", "test", "du", "df", "file", "stat", "which", "diff", "cmp", "shasum", "md5", "realpath", "basename", "dirname", "sleep"];

/// One part of the command (no operators in it): its words checked, with `here` saying whether the cwd is the
/// scratch folder. None: it may write elsewhere. Some(Some(b)): it's a `cd`, after which the cwd is (b) or isn't
/// there; Some(None): anything else allowed.
fn segment(seg: &[Tok], here: bool, piped: bool, ours: &dyn Fn(&str) -> bool) -> Option<Option<bool>> {
    let words = words_of(seg, here, ours)?;
    // Settings before the command (`CARGO_TARGET_DIR=…`): a path in one must be in the scratch folder.
    let k = words.iter().take_while(|w| is_setting(w)).count();
    for w in &words[..k] {
        let (_, v) = w.split_once('=')?;
        if (v.starts_with('/') || v.starts_with('.') || v.starts_with('~')) && !place_ok(v, here, ours) {
            return None;
        }
    }
    let (&cmd, args) = words[k..].split_first()?;
    if cmd == "cd" {
        let to = not_opts(args);
        let [dir] = to.as_slice() else { return None };
        return (!piped).then(|| Some(place_ok(dir, here, ours)));
    }
    command_ok(cmd, args, here, ours).then_some(None)
}

/// The words of a part, each redirection's file checked and left out.
fn words_of<'a>(seg: &'a [Tok], here: bool, ours: &dyn Fn(&str) -> bool) -> Option<Vec<&'a str>> {
    let mut words = vec![];
    let mut it = seg.iter();
    while let Some(t) = it.next() {
        match t {
            Tok::Word(w) => words.push(w.as_str()),
            Tok::Redir(kind) => {
                let Some(Tok::Word(file)) = it.next() else { return None };
                if *kind != "<" && file != "/dev/null" && !place_ok(file, here, ours) {
                    return None;
                }
            }
            Tok::Op(_) => return None,
        }
    }
    Some(words)
}

/// Whether command `cmd` with `args` only writes in the scratch folder.
fn command_ok(cmd: &str, args: &[&str], here: bool, ours: &dyn Fn(&str) -> bool) -> bool {
    let places_ok = |list: &[&str]| list.iter().all(|p| place_ok(p, here, ours));
    let paths = not_opts(args);
    match cmd {
        "mkdir" => {
            // `-m <mode>` takes a word that isn't a folder.
            let mode = args.iter().position(|a| *a == "-m").and_then(|i| args.get(i + 1)).copied();
            let dirs: Vec<&str> = paths.iter().copied().filter(|p| Some(*p) != mode).collect();
            !dirs.is_empty() && places_ok(&dirs)
        }
        "cp" | "ditto" | "rsync" => {
            let odd = args.iter().any(|a| a.starts_with("--remove-source") || a.starts_with("--delete") || *a == "-t" || a.starts_with("--target"));
            !odd && paths.len() >= 2 && paths.last().is_some_and(|d| place_ok(d, here, ours))
        }
        "mv" => paths.len() >= 2 && places_ok(&paths),
        "touch" => !paths.is_empty() && places_ok(&paths),
        "tar" | "bsdtar" | "gtar" => tar_ok(args, here, ours),
        "unzip" => {
            let d = args.iter().position(|a| *a == "-d").and_then(|i| args.get(i + 1));
            d.map_or(here, |d| place_ok(d, here, ours)) && !args.contains(&"-:")
        }
        "git" => git_ok(args, here, ours),
        "find" => !args.iter().any(|a| matches!(*a, "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir") || a.starts_with("-fprint") || a.starts_with("-fls")),
        "sed" => !args.iter().any(|a| a.starts_with("--in-place") || (a.starts_with('-') && !a.starts_with("--") && a.contains('i'))),
        "cargo" | "npm" | "pnpm" | "yarn" | "make" | "go" | "swift" => build_ok(cmd, args, here, ours),
        c => READERS.contains(&c),
    }
}

/// The words that aren't options.
fn not_opts<'a>(list: &[&'a str]) -> Vec<&'a str> {
    list.iter().copied().filter(|a| !a.starts_with('-')).collect()
}

fn is_setting(w: &str) -> bool {
    w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit()))
}

/// What a `tar` call does: its mode ('x' unpack, 'c' pack, 't' list), its archive, and the folders it goes to.
#[derive(Default)]
struct TarCall<'a> {
    mode: Option<char>,
    file: Option<&'a str>,
    dirs: Vec<&'a str>,
}

/// `tar`: unpacking only into the scratch folder (`-C` there, or the cwd there), never with absolute names kept;
/// packing only to a file there (or to its output, piped on); listing anywhere.
fn tar_ok(args: &[&str], here: bool, ours: &dyn Fn(&str) -> bool) -> bool {
    let Some(t) = tar_read(args) else { return false };
    match t.mode {
        Some('x') => if t.dirs.is_empty() { here } else { t.dirs.iter().all(|d| place_ok(d, here, ours)) },
        Some('c') => t.file.is_some_and(|f| f == "-" || place_ok(f, here, ours)),
        Some(_) => true,
        None => false,
    }
}

/// A `tar` call's words read; None for an option that writes outside where it's pointed, or one missing its value.
fn tar_read<'a>(args: &[&'a str]) -> Option<TarCall<'a>> {
    let mut t = TarCall::default();
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        i += 1;
        if let Some(long) = a.strip_prefix("--") {
            let (name, val) = long.split_once('=').map_or((long, None), |(n, v)| (n, Some(v)));
            let mut value = || match val {
                Some(v) => Some(v),
                None => {
                    i += 1;
                    args.get(i - 1).copied()
                }
            };
            match name {
                "extract" | "get" => t.mode = Some('x'),
                "create" | "append" | "update" => t.mode = Some('c'),
                "list" => t.mode = t.mode.or(Some('t')),
                "absolute-names" | "transform" | "xform" | "to-command" | "unlink-first" | "recursive-unlink" | "use-compress-program" => return None,
                "file" => t.file = Some(value()?),
                "directory" | "cd" => t.dirs.push(value()?),
                _ => {}
            }
            continue;
        }
        // A cluster of one-letter options: `-xzf a.tar`, or the first word without its dash (`xzf a.tar`).
        let cluster = a.strip_prefix('-').or((i == 1).then_some(a));
        if let Some(cluster) = cluster.filter(|c| !c.is_empty() && c.chars().all(|ch| ch.is_ascii_alphanumeric())) {
            i = tar_cluster(cluster, args, i, &mut t)?;
        }
    }
    Some(t)
}

/// One cluster of `tar`'s one-letter options read into `t`, those taking a value taking the words from `i` on:
/// where reading goes on.
fn tar_cluster<'a>(cluster: &str, args: &[&'a str], mut i: usize, t: &mut TarCall<'a>) -> Option<usize> {
    for ch in cluster.chars() {
        match ch {
            'x' => t.mode = Some('x'),
            'c' | 'r' | 'u' => t.mode = Some('c'),
            't' => t.mode = t.mode.or(Some('t')),
            'P' | 's' | 'I' | 'U' | 'T' => return None,
            'f' | 'C' | 'b' => {
                let v = args.get(i).copied()?;
                i += 1;
                match ch {
                    'f' => t.file = Some(v),
                    'C' => t.dirs.push(v),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Some(i)
}

/// `git`: only `archive`, written to a file in the scratch folder or to its output (piped on). `-C <dir>` (where
/// it reads from) may be anywhere.
fn git_ok(args: &[&str], here: bool, ours: &dyn Fn(&str) -> bool) -> bool {
    let mut i = 0;
    while i < args.len() && args[i].starts_with('-') {
        if matches!(args[i], "-C" | "-c" | "--git-dir" | "--work-tree") {
            i += 1;
        }
        i += 1;
    }
    if args.get(i) != Some(&"archive") {
        return false;
    }
    let rest = &args[i + 1..];
    let mut j = 0;
    while j < rest.len() {
        let a = rest[j];
        if a == "--remote" || a.starts_with("--remote=") || a == "--exec" || a.starts_with("--exec=") {
            return false;
        }
        let out = if a == "-o" || a == "--output" {
            j += 1;
            match rest.get(j) {
                Some(f) => Some(*f),
                None => return false,
            }
        } else {
            a.strip_prefix("--output=").or_else(|| a.strip_prefix("-o").filter(|f| !f.is_empty()))
        };
        if let Some(f) = out {
            if !place_ok(f, here, ours) {
                return false;
            }
        }
        j += 1;
    }
    true
}

/// A build: run with the cwd in the scratch folder, and any folder it's pointed at there too.
fn build_ok(cmd: &str, args: &[&str], here: bool, ours: &dyn Fn(&str) -> bool) -> bool {
    if !here {
        return false;
    }
    let sub = args.iter().find(|a| !a.starts_with('-')).copied().unwrap_or("");
    let fine = match cmd {
        "cargo" => matches!(sub, "build" | "b" | "check" | "c" | "test" | "t" | "fetch" | "metadata" | "tree"),
        "npm" | "pnpm" | "yarn" => matches!(sub, "install" | "i" | "ci" | "test" | "build" | "run" | "" ),
        "go" => matches!(sub, "build" | "test" | "vet"),
        "swift" => matches!(sub, "build" | "test"),
        "make" => true,
        _ => false,
    };
    // What a target or script is named for: putting things outside the folder.
    // (`npm install` / `npm i` only fetches into the folder; `make install` puts things outside it.)
    let fetch = |a: &str| cmd != "make" && matches!(a, "install" | "i");
    let outward = args.iter().any(|a| !fetch(a) && ["install", "deploy", "publish", "release", "push", "upload"].iter().any(|w| a.contains(w)));
    if !fine || outward {
        return false;
    }
    // Options that point it at other folders: each must be in the scratch folder.
    const POINTERS: &[&str] = &["--manifest-path", "--target-dir", "--prefix", "-C", "--directory", "--cwd", "--dir", "--package-path", "--scratch-path", "-o", "--out-dir", "--artifact-dir", "--global", "-g", "--location"];
    for (i, a) in args.iter().enumerate() {
        let (name, val) = a.split_once('=').map_or((*a, None), |(n, v)| (n, Some(v)));
        if matches!(name, "-g" | "--global" | "--location") {
            return false;
        }
        if POINTERS.contains(&name) {
            let Some(v) = val.or_else(|| args.get(i + 1).copied()) else { return false };
            if !place_ok(v, here, ours) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(sid: &str) -> String {
        // SAFETY: getuid has no preconditions and can't fail.
        format!("/private/tmp/claude-{}/-Users-me-app/{sid}/scratchpad", unsafe { libc::getuid() })
    }

    fn ok(command: &str) -> bool {
        only_writes_in_scratch(command, "/Users/me/app", "s1")
    }

    #[test]
    fn making_copying_unpacking_and_building_in_its_own_scratch_folder_is_allowed() {
        let p = pad("s1");
        for c in [
            format!("mkdir -p {p}/src"),
            format!("git -C /Users/me/app archive HEAD | tar -x -C {p}/src"),
            format!("git archive --format=tar -o {p}/app.tar HEAD"),
            format!("mkdir -p {p}/b && tar xf {p}/app.tar -C {p}/b"),
            format!("cd {p}/src && cargo build -q 2>&1 | tail -5"),
            format!("cd '{p}/src' && npm ci && npm run build > {p}/build.log 2>&1"),
            format!("cp -R /Users/me/app/ui {p}/ui"),
            format!("ls {p}; cat {p}/build.log | grep error"),
            format!("cd {p} && tar czf out.tgz src"),
            format!("mkdir -p /tmp/claude-{}/-Users-me-app/s1/scratchpad/x", unsafe { libc::getuid() }),
        ] {
            assert!(ok(&c), "should run without a prompt: {c}");
        }
    }

    #[test]
    fn deletes_pushes_releases_and_writes_elsewhere_still_prompt() {
        let p = pad("s1");
        let other = pad("s2");
        for c in [
            format!("rm -rf {p}/src"),
            format!("mkdir -p {p}/a && git push origin main"),
            format!("cd {p} && scripts/release.sh 1.0"),
            format!("mkdir -p {other}/x"),
            format!("mkdir -p {p}/../../s2/scratchpad"),
            "mkdir -p /Users/me/app/out".to_string(),
            format!("cp {p}/a /Users/me/app/a"),
            "echo hi > /Users/me/app/x".to_string(),
            format!("cd {p}; cargo build"),
            format!("cd {p} || cargo build"),
            format!("cargo build --manifest-path {p}/Cargo.toml"),
            format!("cd {p} && cargo publish"),
            format!("tar xf {p}/a.tar"),
            format!("tar xPf {p}/a.tar -C {p}"),
            format!("mkdir $HOME/x {p}/y"),
            format!("mkdir $(echo /etc) {p}/y"),
            format!("cd {p} && cargo build & rm -rf /"),
            format!("cd {p} && make; make install"),
            "git archive HEAD -o /Users/me/out.tar".to_string(),
            format!("cd {p} && npm install -g thing"),
            format!("find {p} -delete"),
            format!("sed -i '' s/a/b/ {p}/f"),
            format!("mkdir {p}/{{.,.}}./x"),
            format!("cd {p} && npm run deploy"),
            format!("cd {p} && make install"),
        ] {
            assert!(!ok(&c), "should still prompt: {c}");
        }
        assert!(!only_writes_in_scratch(&format!("mkdir {p}/x"), "/", ""), "no session id: nothing is its own");
    }

    #[test]
    fn a_session_started_in_its_scratch_folder_builds_there() {
        let p = pad("s1");
        assert!(only_writes_in_scratch("mkdir out && cargo build", &format!("{p}/src"), "s1"));
        assert!(!only_writes_in_scratch("mkdir out", "/Users/me/app", "s1"));
    }
}
