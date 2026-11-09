//! The files a shell command writes, as far as a quick read of its words can tell (docs/dev/cue-teams.md §7): for a
//! teammate working in its lead's own folder, where no worktree of its own says what changed. Redirects (`> f`,
//! `>> f`, `&> f`), `tee`, `sed -i`, `mv` (both ends), `cp` (its target), `touch`, `rm` (not `-r`), `dd of=`,
//! `git checkout … -- f`, `git restore`, `git mv` and `git rm`, through `cd` and `&&`, `;`, `|`, `||`. A word with
//! `$`, a glob or `~` in it, or anything a command substitution or a heredoc's body says, is left out: best effort,
//! never a guess.
//!
//! Pure: a command's text in, paths out (relative to where it started, or absolute; normalized).

/// One piece of a command line.
#[derive(Debug, PartialEq)]
enum Tok {
    /// A word, its quotes taken off; `plain` when nothing in it needs the shell to expand it.
    Word { text: String, plain: bool },
    /// `|`, `||`, `&&`, `;`, `&`, a newline, `(` or `)`: where one simple command ends.
    Sep,
    /// A redirect: `>`, `>>`, `>|`, `&>`, `&>>`, `<`, `<<`, `<<-`, `<<<`, `>&`, each maybe after a file descriptor.
    Redir(String),
}

/// `command`'s words and operators, a heredoc's body and comments skipped.
fn lex(command: &str) -> Vec<Tok> {
    let mut l = Lexer { cs: command.chars().collect(), plain: true, ..Default::default() };
    while l.i < l.cs.len() {
        l.step();
        l.i += 1;
    }
    l.end_word();
    l.out
}

/// Reading a command line, a character at a time.
#[derive(Default)]
struct Lexer {
    cs: Vec<char>,
    i: usize,
    out: Vec<Tok>,
    /// The word being read, whether it's plain, and whether one is under way (an empty `''` is a word).
    word: String,
    plain: bool,
    in_word: bool,
    /// Heredocs whose body starts at the next newline: (delimiter, `<<-`); and one whose delimiter is the next word.
    heredocs: Vec<(String, bool)>,
    want_delim: Option<bool>,
}

impl Lexer {
    fn at(&self, n: usize) -> Option<char> {
        self.cs.get(self.i + n).copied()
    }

    /// The character at `i`, and what it starts.
    fn step(&mut self) {
        let c = self.cs[self.i];
        match c {
            '\'' | '"' => self.quoted(c),
            '\\' => {
                self.in_word = true;
                self.i += 1;
                if let Some(n) = self.at(0).filter(|n| *n != '\n') {
                    self.word.push(n);
                }
            }
            '$' if self.at(1) == Some('(') => self.substitution(),
            '`' => {
                self.in_word = true;
                self.plain = false;
                self.i += 1;
                while self.at(0).is_some_and(|n| n != '`') {
                    self.i += 1;
                }
            }
            '#' if !self.in_word => {
                while self.at(1).is_some_and(|n| n != '\n') {
                    self.i += 1;
                }
            }
            ' ' | '\t' => self.end_word(),
            '\n' => {
                self.end_word();
                self.out.push(Tok::Sep);
                self.heredoc_bodies();
            }
            ';' | '|' | '(' | ')' | '&' if !(c == '&' && self.at(1) == Some('>')) => {
                self.end_word();
                if (c == '|' || c == '&') && self.at(1) == Some(c) {
                    self.i += 1;
                }
                self.out.push(Tok::Sep);
            }
            '&' | '>' | '<' => self.redirect(c),
            _ => {
                if matches!(c, '*' | '?' | '[' | '$' | '{') || (c == '~' && !self.in_word) {
                    self.plain = false;
                }
                self.in_word = true;
                self.word.push(c);
            }
        }
    }

    /// The word read so far, as a word (or the delimiter of the heredoc before it).
    fn end_word(&mut self) {
        if !self.in_word {
            return;
        }
        let w = std::mem::take(&mut self.word);
        if let Some(dash) = self.want_delim.take() {
            self.heredocs.push((w, dash));
        } else {
            self.out.push(Tok::Word { text: w, plain: self.plain });
        }
        self.plain = true;
        self.in_word = false;
    }

    /// A quoted stretch: nothing in single quotes is expanded; in double quotes `$` and `` ` `` are.
    fn quoted(&mut self, q: char) {
        self.in_word = true;
        self.i += 1;
        while let Some(c) = self.at(0).filter(|c| *c != q) {
            if q == '"' && c == '\\' && self.at(1).is_some_and(|n| matches!(n, '"' | '\\' | '$' | '`')) {
                self.i += 1;
            } else if q == '"' && matches!(c, '$' | '`') {
                self.plain = false;
            }
            self.word.push(self.cs[self.i]);
            self.i += 1;
        }
    }

    /// `$(…)`: its words are its own command's, not this one's.
    fn substitution(&mut self) {
        self.in_word = true;
        self.plain = false;
        let mut depth = 0;
        while let Some(c) = self.at(0) {
            match c {
                '(' => depth += 1,
                ')' if depth == 1 => return,
                ')' => depth -= 1,
                _ => {}
            }
            self.word.push(c);
            self.i += 1;
        }
    }

    /// Each heredoc's body, from the newline at `i`: the lines up to its delimiter, skipped.
    fn heredoc_bodies(&mut self) {
        for (delim, dash) in std::mem::take(&mut self.heredocs) {
            loop {
                let start = self.i + 1;
                let end = (start..self.cs.len()).find(|&j| self.cs[j] == '\n').unwrap_or(self.cs.len());
                let line: String = self.cs[start.min(self.cs.len())..end].iter().collect();
                self.i = end;
                let line = if dash { line.trim_start_matches('\t') } else { line.as_str() };
                if line.trim_end() == delim || self.i >= self.cs.len() {
                    break;
                }
            }
        }
    }

    /// A redirect starting with `c`, a file descriptor just before it (`2>`) part of it.
    fn redirect(&mut self, c: char) {
        let fd = self.in_word && self.plain && !self.word.is_empty() && self.word.chars().all(|d| d.is_ascii_digit());
        let prefix = if fd { std::mem::take(&mut self.word) } else { String::new() };
        if fd {
            self.in_word = false;
        }
        self.end_word();
        let rest: String = self.cs[self.i + 1..self.cs.len().min(self.i + 3)].iter().collect();
        let more = match c {
            '&' => if rest.starts_with(">>") { ">>" } else { ">" },
            '>' => ["&", ">", "|"].into_iter().find(|m| rest.starts_with(m)).unwrap_or(""),
            _ => ["<<", "<-", "<", "&"].into_iter().find(|m| rest.starts_with(m)).unwrap_or(""),
        };
        let op = format!("{prefix}{c}{more}");
        self.i += more.len();
        let bare = op.trim_start_matches(|d: char| d.is_ascii_digit());
        if bare == "<<" || bare == "<<-" {
            self.want_delim = Some(bare == "<<-");
        } else {
            self.out.push(Tok::Redir(op));
        }
    }
}

/// The files `command` writes: relative to the folder it starts in (`cd` followed), or absolute; never /dev's.
pub fn writes(command: &str) -> Vec<String> {
    let toks = lex(command);
    let mut found: Vec<String> = vec![];
    // Where it is now, relative to where it started ("" there); None once a `cd` went somewhere it can't tell.
    let mut here: Option<String> = Some(String::new());
    for cmd in toks.split(|t| *t == Tok::Sep) {
        let mut argv: Vec<(&str, bool)> = vec![];
        let mut targets: Vec<&str> = vec![];
        let mut it = cmd.iter();
        while let Some(t) = it.next() {
            match t {
                Tok::Word { text, plain } => argv.push((text, *plain)),
                Tok::Redir(op) => {
                    let Some(Tok::Word { text, plain }) = it.next() else { continue };
                    // `>&` with a descriptor after it (`2>&1`) is no file.
                    if op.contains('>') && !op.ends_with(">&") && *plain {
                        targets.push(text);
                    }
                }
                Tok::Sep => {}
            }
        }
        // What runs: past `VAR=x` and the commands that run another.
        let mut a = 0;
        while a < argv.len() && (is_assignment(argv[a].0) || matches!(argv[a].0, "sudo" | "env" | "command" | "nohup" | "time" | "exec" | "builtin")) {
            a += 1;
        }
        let argv = &argv[a..];
        for t in targets {
            wrote(&mut found, here.as_deref(), t);
        }
        let Some(&(name, true)) = argv.first() else { continue };
        let args = &argv[1..];
        let name = name.rsplit('/').next().unwrap_or(name);
        match name {
            "cd" | "pushd" => {
                here = match args.first() {
                    Some(&(d, true)) if d != "-" => resolve(here.as_deref(), d).filter(|d| !d.starts_with("/dev")),
                    _ => None,
                };
            }
            _ => {
                for p in files_of(name, args) {
                    wrote(&mut found, here.as_deref(), &p);
                }
            }
        }
    }
    found
}

/// `p` (from `here`) into `found`, once.
fn wrote(found: &mut Vec<String>, here: Option<&str>, p: &str) {
    if let Some(f) = resolve(here, p) {
        if !found.contains(&f) {
            found.push(f);
        }
    }
}

fn is_assignment(w: &str) -> bool {
    w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit()))
}

/// The files command `name` with `args` writes, by what it is.
fn files_of(name: &str, args: &[(&str, bool)]) -> Vec<String> {
    let recursive = args.iter().any(|(w, _)| (w.starts_with('-') && !w.starts_with("--") && w.contains(['r', 'R'])) || *w == "--recursive");
    match name {
        "tee" => plain(operands(args, &[])),
        "touch" => plain(operands(args, &["-d", "-t", "-r"])),
        // A whole folder isn't told file by file.
        "rm" if !recursive => plain(operands(args, &[])),
        "dd" => args.iter().filter(|(_, p)| *p).filter_map(|(w, _)| w.strip_prefix("of=").map(String::from)).collect(),
        "sed" | "gsed" => sed_files(args),
        "mv" => {
            // Both ends: each source is gone from where it was, and lands at the target.
            let ops = operands(args, &["-t", "-S", "--suffix", "--target-directory"]);
            let sources = if args.iter().any(|(w, _)| *w == "-t") { &ops[..] } else { &ops[..ops.len().saturating_sub(1)] };
            let mut out = plain(sources.to_vec());
            out.extend(target(args, &ops));
            out
        }
        "cp" => target(args, &operands(args, &["-t", "-S", "--suffix", "--target-directory"])),
        "git" => git_files(args),
        _ => vec![],
    }
}

/// A command's operands: the words that aren't options (all of them after `--`), skipping the values of `with_value`.
fn operands(args: &[(&str, bool)], with_value: &[&str]) -> Vec<(String, bool)> {
    let mut out = vec![];
    let mut ended = false;
    let mut i = 0;
    while i < args.len() {
        let (w, plain) = args[i];
        if ended || !w.starts_with('-') || w == "-" {
            out.push((w.to_string(), plain));
        } else if w == "--" {
            ended = true;
        } else if with_value.contains(&w) {
            i += 1;
        }
        i += 1;
    }
    out
}

/// The plain ones of `words`.
fn plain(words: Vec<(String, bool)>) -> Vec<String> {
    words.into_iter().filter(|(_, p)| *p).map(|(w, _)| w).collect()
}

/// `cp`/`mv`'s target: its last operand, or `-t DIR`; a target that ends in `/` takes each source's name.
fn target(args: &[(&str, bool)], ops: &[(String, bool)]) -> Vec<String> {
    let t_dir = args.windows(2).find(|w| w[0].0 == "-t").map(|w| w[1]);
    let (dir, sources) = match t_dir {
        Some(d) => (Some(d), ops),
        None => match ops.split_last() {
            Some((last, rest)) if !rest.is_empty() => (Some((last.0.as_str(), last.1)), rest),
            _ => (None, ops),
        },
    };
    let Some((dir, true)) = dir else { return vec![] };
    if t_dir.is_some() || dir.ends_with('/') {
        sources.iter().filter(|(_, p)| *p).map(|(s, _)| format!("{}/{}", dir.trim_end_matches('/'), s.trim_end_matches('/').rsplit('/').next().unwrap_or(s))).collect()
    } else {
        vec![dir.to_string()]
    }
}

/// `sed`'s files when it edits them in place (`-i`, `-i.bak`, `-Ei`, `--in-place`): its operands after the script.
fn sed_files(args: &[(&str, bool)]) -> Vec<String> {
    let in_place = |w: &str| w.starts_with("-i") || w.starts_with("--in-place") || (w.starts_with('-') && !w.starts_with("--") && w[1..].chars().all(|c| c.is_ascii_alphabetic()) && w[1..].split(['e', 'f']).next().is_some_and(|s| s.contains('i')));
    if !args.iter().any(|(w, _)| in_place(w)) {
        return vec![];
    }
    // BSD sed's `-i ''`: the empty word is its backup suffix, not a file.
    let kept: Vec<(&str, bool)> = args.iter().enumerate().filter(|(n, a)| !(a.0.is_empty() && *n > 0 && args[n - 1].0 == "-i")).map(|(_, a)| *a).collect();
    let script_given = kept.iter().any(|(w, _)| matches!(*w, "-e" | "-f" | "--expression" | "--file") || w.starts_with("--expression=") || w.starts_with("--file="));
    let ops = operands(&kept, &["-e", "-f", "-l", "--expression", "--file"]);
    plain(ops.into_iter().skip(if script_given { 0 } else { 1 }).collect())
}

/// The files a `git` command writes over, where its words name them.
fn git_files(args: &[(&str, bool)]) -> Vec<String> {
    // Past git's own options (`-C dir` moves where its paths are from).
    let mut i = 0;
    let mut dir = String::new();
    while i < args.len() && args[i].0.starts_with('-') {
        if matches!(args[i].0, "-C" | "-c" | "--git-dir" | "--work-tree") {
            if args[i].0 == "-C" {
                if let Some(&(d, true)) = args.get(i + 1) {
                    dir = format!("{}/", d.trim_end_matches('/'));
                }
            }
            i += 1;
        }
        i += 1;
    }
    let Some(&(sub, _)) = args.get(i) else { return vec![] };
    let rest = &args[i + 1..];
    let after_dashes = || rest.iter().skip_while(|(w, _)| *w != "--").skip(1).filter(|(_, p)| *p).map(|(w, _)| w.to_string()).collect::<Vec<_>>();
    let operands = |with_value: &[&str]| plain(operands(rest, with_value));
    let files = match sub {
        // `git checkout <branch>` moves the whole folder: only its `-- paths` form names files.
        "checkout" => after_dashes(),
        "restore" => operands(&["-s", "--source"]),
        "mv" => operands(&[]),
        "rm" if !rest.iter().any(|(w, _)| *w == "-r" || *w == "--cached") => operands(&[]),
        _ => vec![],
    };
    files.into_iter().map(|f| if f.starts_with('/') { f } else { format!("{dir}{f}") }).collect()
}

/// `p` from `here` (relative to the start, or absolute), with `.` and `..` worked out; None when it can't be told.
fn resolve(here: Option<&str>, p: &str) -> Option<String> {
    if p.is_empty() || p.starts_with("/dev/") || p == "/dev" {
        return None;
    }
    let joined = if p.starts_with('/') {
        p.to_string()
    } else {
        let here = here?;
        if here.is_empty() { p.to_string() } else { format!("{}/{p}", here.trim_end_matches('/')) }
    };
    let abs = joined.starts_with('/');
    let mut parts: Vec<&str> = vec![];
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                // Above where it started: a path outside its folder, told as it was written.
                if parts.pop().is_none() && !abs {
                    return Some(joined);
                }
            }
            _ => parts.push(part),
        }
    }
    let mut out = parts.join("/");
    if abs {
        out.insert(0, '/');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::writes;

    fn w(cmd: &str) -> Vec<String> {
        writes(cmd)
    }

    #[test]
    fn redirects_write_their_file_and_a_file_descriptor_or_dev_null_isnt_one() {
        assert_eq!(w("printf 'x\\n' > b.js"), ["b.js"]);
        assert_eq!(w("echo hi >>log.txt"), ["log.txt"]);
        assert_eq!(w("cargo test 2>err.txt >/dev/null"), ["err.txt"]);
        assert_eq!(w("make &> build.log"), ["build.log"]);
        assert_eq!(w("cmd 2>&1 | head"), Vec::<String>::new());
        assert_eq!(w("cmd >| forced.txt"), ["forced.txt"]);
        assert_eq!(w("echo '>' not-a-file"), Vec::<String>::new(), "quoted");
        assert_eq!(w("sort < in.txt > out.txt"), ["out.txt"], "reads aren't writes");
        assert_eq!(w("echo x > \"$OUT\""), Vec::<String>::new(), "a variable: unknown");
    }

    #[test]
    fn a_heredocs_body_is_not_read_as_commands() {
        let cmd = "cat > notes.md <<'EOF'\nline with > arrow.txt\nmv a b\nEOF\necho done > after.txt";
        assert_eq!(w(cmd), ["notes.md", "after.txt"]);
    }

    #[test]
    fn tee_sed_in_place_mv_cp_touch_rm_and_dd() {
        assert_eq!(w("echo x | tee -a one.txt two.txt"), ["one.txt", "two.txt"]);
        assert_eq!(w("sed -i 's/a/b/' src/a.rs src/b.rs"), ["src/a.rs", "src/b.rs"]);
        assert_eq!(w("sed -i '' 's/a/b/' a.rs"), ["a.rs"], "BSD sed's empty suffix");
        assert_eq!(w("sed -i.bak -e 's/a/b/' -e 's/c/d/' a.rs"), ["a.rs"]);
        assert_eq!(w("sed -Ei 's/a/b/' a.rs"), ["a.rs"]);
        assert_eq!(w("sed 's/a/b/' a.rs"), Vec::<String>::new(), "not in place");
        assert_eq!(w("mv old.txt new.txt"), ["old.txt", "new.txt"], "a move changes both ends");
        assert_eq!(w("mv a.txt b.txt dest/"), ["a.txt", "b.txt", "dest/a.txt", "dest/b.txt"]);
        assert_eq!(w("cp -r tmpl.txt copy.txt"), ["copy.txt"]);
        assert_eq!(w("cp a.txt lib/"), ["lib/a.txt"]);
        assert_eq!(w("cp -t lib a.txt b.txt"), ["lib/a.txt", "lib/b.txt"]);
        assert_eq!(w("touch -d now x.txt"), ["x.txt"]);
        assert_eq!(w("rm -f gone.txt"), ["gone.txt"]);
        assert_eq!(w("rm -rf target"), Vec::<String>::new(), "a whole folder: not told file by file");
        assert_eq!(w("dd if=/dev/zero of=blob.bin bs=1 count=1"), ["blob.bin"]);
    }

    #[test]
    fn git_checkout_and_restore_name_their_files_a_branch_switch_doesnt() {
        assert_eq!(w("git checkout -- a.rs b.rs"), ["a.rs", "b.rs"]);
        assert_eq!(w("git checkout HEAD~1 -- a.rs"), ["a.rs"]);
        assert_eq!(w("git checkout main"), Vec::<String>::new());
        assert_eq!(w("git restore --source HEAD a.rs"), ["a.rs"]);
        assert_eq!(w("git -C sub restore a.rs"), ["sub/a.rs"]);
        assert_eq!(w("git mv a.rs b.rs"), ["a.rs", "b.rs"]);
        assert_eq!(w("git rm --cached a.rs"), Vec::<String>::new(), "the file stays");
    }

    #[test]
    fn cd_and_chains_are_followed_and_paths_normalized() {
        assert_eq!(w("cd ui && printf x > a.js; echo y >> ./b.js"), ["ui/a.js", "ui/b.js"]);
        assert_eq!(w("cd ui && cd .. && echo x > top.txt"), ["top.txt"]);
        assert_eq!(w("cd \"$DIR\" && echo x > lost.txt"), Vec::<String>::new(), "where it is can't be told");
        assert_eq!(w("echo x > /abs/path.txt"), ["/abs/path.txt"]);
        assert_eq!(w("FOO=1 sudo tee out.txt < in"), ["out.txt"]);
        assert_eq!(w("echo x > ../outside.txt"), ["../outside.txt"]);
        assert_eq!(w("echo $(cat a > inner.txt) > outer.txt"), ["outer.txt"], "a substitution's own words aren't");
        assert_eq!(w("ls # > not.txt"), Vec::<String>::new(), "a comment");
        assert_eq!(w("cat *.txt > all.txt"), ["all.txt"]);
        assert_eq!(w("echo x > *.txt"), Vec::<String>::new(), "a glob target");
    }
}
