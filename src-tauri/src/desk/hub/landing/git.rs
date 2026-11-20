//! git for the Desk (split from desk/hub/landing/commit.rs): git run with the repository's own filters off, its own words when
//! it refuses, and the word check of a diff's added lines.

/// git in `dir` on `machine` ("" here), with the repository's own filters off (safe_git.rs): its output, when it
/// worked.
pub(crate) fn git(machine: &str, dir: &str, env: &[(&str, &str)], args: &[&str]) -> Option<String> {
    let filters = crate::safe_git::no_repo_filters(machine, dir);
    crate::safe_git::run_env(machine, dir, &filters, env, args).ok().filter(|(_, code)| *code == 0).map(|(o, _)| o)
}

/// `git`, and when it fails, why in git's own words ("" when it said nothing): for a step whose failure you're shown.
pub(crate) fn git_said(machine: &str, dir: &str, env: &[(&str, &str)], args: &[&str]) -> Result<String, String> {
    let filters = crate::safe_git::no_repo_filters(machine, dir);
    crate::safe_git::run_said(machine, dir, &filters, env, args)
}

/// `what` failed, and git's own words for it after a colon when it said any.
pub(crate) fn because(what: String, said: &str) -> String {
    if said.is_empty() { what } else { format!("{what}: {said}") }
}

/// The first of `words` (any case) in `diff`'s added lines, or in the name of a file it adds: the word, and where
/// ("path: the line").
pub(crate) fn word_in(diff: &str, words: &[String]) -> Option<(String, String)> {
    let words: Vec<(String, String)> = words.iter().filter(|w| !w.trim().is_empty()).map(|w| (w.clone(), w.to_lowercase())).collect();
    let find = |text: &str| {
        let low = text.to_lowercase();
        words.iter().find(|(_, l)| low.contains(l.as_str())).map(|(w, _)| w.clone())
    };
    let mut file = String::new();
    let mut fresh = false;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            file = path.strip_prefix("b/").unwrap_or(path).to_string();
            if fresh {
                if let Some(w) = find(&file) {
                    return Some((w, format!("the name {file}")));
                }
            }
            continue;
        }
        if line.starts_with("--- ") {
            fresh = line == "--- /dev/null";
            continue;
        }
        if let Some(added) = line.strip_prefix('+') {
            if let Some(w) = find(added) {
                let shown: String = added.trim().chars().take(80).collect();
                return Some((w, format!("{file}: {shown}")));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::word_in;

    fn w(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_word_in_an_added_line_is_found_in_any_case_and_said_where() {
        let diff = "diff --git a/ui/a.js b/ui/a.js\n--- a/ui/a.js\n+++ b/ui/a.js\n@@ -1 +1 @@\n-old line with Secret\n+const x = \"Secret plan\";\n";
        assert_eq!(word_in(diff, &w(&["secret"])), Some(("secret".into(), "ui/a.js: const x = \"Secret plan\";".into())));
    }

    #[test]
    fn a_removed_line_or_an_old_files_name_is_fine_a_new_files_name_isnt() {
        let removed = "--- a/secret.txt\n+++ b/secret.txt\n@@ -1 +0,0 @@\n-the secret\n";
        assert_eq!(word_in(removed, &w(&["secret"])), None);
        let fresh = "--- /dev/null\n+++ b/notes/secret.md\n@@ -0,0 +1 @@\n+hello\n";
        assert_eq!(word_in(fresh, &w(&["SECRET"])), Some(("SECRET".into(), "the name notes/secret.md".into())));
    }

    #[test]
    fn no_words_finds_nothing() {
        assert_eq!(word_in("--- a/x\n+++ b/x\n+anything\n", &w(&[" ", ""])), None);
    }
}
