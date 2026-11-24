//! The Desk block a lead reads before each message (#818), as lines it can be sent whole or as the lines that changed.
//! Each line has a key (its section and what it's of: "it|42", "s|<sid>"), so two blocks can be compared line by
//! line; `diff` gives the changes from one to the next, and `apply` rebuilds the next from the last and those changes.
//! Which a message carries, whole or changes only, and when the rules go with it: `Sent` (one per Desk).

/// A block's lines in order: (key, text). Keys are unique within a block.
pub(crate) type Lines = Vec<(String, String)>;

/// One change from one block to the next: a line put in (new, changed, or moved) right after the line keyed `after`
/// (None: first), or a line gone.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Op {
    Put { key: String, text: String, after: Option<String> },
    Gone { key: String },
}

/// The changes that turn `old` into `new`: every line gone first, then the lines put in, in `new`'s order. Only a line
/// that's new, changed, or not after the line it follows in `new` is put in. `apply(old, diff(old, new)) == new`.
pub(crate) fn diff(old: &Lines, new: &Lines) -> Vec<Op> {
    let mut ops: Vec<Op> = old.iter().filter(|(k, _)| !new.iter().any(|(n, _)| n == k)).map(|(k, _)| Op::Gone { key: k.clone() }).collect();
    let mut cur: Lines = old.iter().filter(|(k, _)| new.iter().any(|(n, _)| n == k)).cloned().collect();
    // Invariant: after line i, `cur` starts with new[..=i].
    for (i, (k, t)) in new.iter().enumerate() {
        if cur.get(i).is_some_and(|(ck, ct)| ck == k && ct == t) {
            continue;
        }
        let op = Op::Put { key: k.clone(), text: t.clone(), after: i.checked_sub(1).map(|p| new[p].0.clone()) };
        put(&mut cur, &op);
        ops.push(op);
    }
    ops
}

/// `old` with `ops` applied in order: what a lead that read `old` and then the changes has.
#[cfg(test)]
pub(crate) fn apply(old: &Lines, ops: &[Op]) -> Lines {
    let mut cur = old.clone();
    for op in ops {
        match op {
            Op::Gone { key } => cur.retain(|(k, _)| k != key),
            Op::Put { .. } => put(&mut cur, op),
        }
    }
    cur
}

fn put(cur: &mut Lines, op: &Op) {
    let Op::Put { key, text, after } = op else { return };
    cur.retain(|(k, _)| k != key);
    let at = match after {
        None => 0,
        Some(a) => cur.iter().position(|(k, _)| k == a).map_or(cur.len(), |p| p + 1),
    };
    cur.insert(at, (key.clone(), text.clone()));
}

/// The block's lines as the lead reads them whole.
pub(crate) fn whole(lines: &Lines) -> String {
    let mut out = String::new();
    for (_, t) in lines {
        out.push_str(t);
        out.push('\n');
    }
    out
}

/// A key's section: what comes before its "|".
fn section(key: &str) -> &str {
    key.split('|').next().unwrap_or(key)
}

/// The heading a section's lines go under in a block of changes ("" for the opening lines).
fn heading_of(lines: &Lines, key: &str) -> Option<String> {
    let at = lines.iter().position(|(k, _)| k == key)?;
    lines[..at].iter().rev().find(|(k, _)| section(k) == "h").map(|(_, t)| t.trim_end_matches(':').trim().to_string())
}

/// The changes from `old` to `new` as the lead reads them: each changed or new line under the heading it's under now,
/// then what's no longer listed. A line only moved within its heading isn't said, but in Needs routing (your order).
pub(crate) fn changes(old: &Lines, new: &Lines, ops: &[Op]) -> String {
    let mut out = String::new();
    let mut under: Option<Option<String>> = None;
    let mut said = 0;
    for op in ops {
        let Op::Put { key, text, .. } = op else { continue };
        if section(key) == "h" || section(key) == "gap" {
            continue;
        }
        let was = old.iter().find(|(k, _)| k == key);
        let (now_h, was_h) = (heading_of(new, key), heading_of(old, key));
        let ordered = now_h.as_deref().is_some_and(|h| h.starts_with("Needs routing"));
        if was.is_some_and(|(_, t)| t == text) && now_h == was_h && !ordered {
            continue;
        }
        if under.as_ref() != Some(&now_h) {
            if let Some(h) = &now_h {
                out.push_str(&format!("{h}:\n"));
            }
            under = Some(now_h);
        }
        out.push_str(text);
        out.push('\n');
        said += 1;
    }
    let gone: Vec<String> = ops.iter().filter_map(|op| match op {
        Op::Gone { key } => gone_words(old, key),
        _ => None,
    }).collect();
    if !gone.is_empty() {
        out.push_str(&format!("No longer listed: {}\n", gone.join("; ")));
        said += 1;
    }
    if said == 0 {
        out.push_str("Nothing changed.\n");
    }
    out
}

/// How a line no longer in the block is named: its item, its session, else its words.
fn gone_words(old: &Lines, key: &str) -> Option<String> {
    let (_, text) = old.iter().find(|(k, _)| k == key)?;
    match section(key) {
        "h" | "gap" => None,
        "it" => Some(format!("#{} (done, dropped or moved)", &key[3..])),
        "s" => Some(format!("session {}", text.trim_start_matches("- ").split(" (").next().unwrap_or(text))),
        _ => Some(format!("\"{}\"", text.trim_start_matches("- "))),
    }
}

/// How often a whole block goes again, in messages, when nothing else calls for one.
pub(crate) const FULL_EVERY: u32 = 20;

/// What a Desk's lead was last sent of its block (#818): which lead, the lines as it has them, how many messages
/// carried changes only since the last whole one, and when that whole one with the rules went. Not saved: after a
/// restart, the next message carries the whole block and the rules.
#[derive(Clone, Debug, Default)]
pub(crate) struct Sent {
    pub sid: String,
    pub base: Lines,
    pub since_full: u32,
    /// When the rules last went with a block (ms): a compaction after it sends them again.
    pub rules_at: u64,
    /// The lead asked for the whole block (SHOW: desk): its next message carries it.
    pub full_next: bool,
}

/// What a message to a lead carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    /// The whole block, with the rules.
    First,
    /// The whole block.
    Whole,
    /// Only the lines that changed.
    Changes,
}

/// What lead `sid` is sent next, by what it was sent (`sent`): the whole block and the rules to a lead not sent one
/// yet (a new lead, one after a hand-off, after a restart) or one that compacted since (`compacted_ms`); the whole
/// block when it asked, or every FULL_EVERY messages; else the changes.
pub(crate) fn kind_for(sent: Option<&Sent>, sid: &str, compacted_ms: u64) -> Kind {
    match sent {
        Some(s) if s.sid == sid && compacted_ms <= s.rules_at => {
            if s.full_next || s.since_full + 1 >= FULL_EVERY || s.base.is_empty() { Kind::Whole } else { Kind::Changes }
        }
        _ => Kind::First,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(pairs: &[(&str, &str)]) -> Lines {
        pairs.iter().map(|(k, t)| (k.to_string(), t.to_string())).collect()
    }

    /// A small random number source (xorshift), so the test runs the same each time.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    /// A random block: headings, items and sessions in a random order, some texts changed.
    fn random_block(r: &mut Rng) -> Lines {
        let mut keys: Vec<String> = vec!["h|needs".into(), "h|open".into(), "h|routes".into()];
        for i in 0..r.next(12) {
            keys.push(format!("it|{}", r.next(30) + i * 0));
        }
        for _ in 0..r.next(8) {
            keys.push(format!("s|{}", r.next(15)));
        }
        keys.dedup();
        let mut seen = std::collections::HashSet::new();
        keys.retain(|k| seen.insert(k.clone()));
        for i in (1..keys.len()).rev() {
            keys.swap(i, r.next(i + 1));
        }
        keys.into_iter().map(|k| { let t = format!("- {k} state {}", r.next(3)); (k, t) }).collect()
    }

    #[test]
    fn applying_the_changes_in_order_always_rebuilds_the_whole_block() {
        let mut r = Rng(0x9e3779b97f4a7c15);
        for _ in 0..300 {
            let mut lead_has: Lines = Vec::new();
            let mut last: Lines = Vec::new();
            for _ in 0..25 {
                let next = random_block(&mut r);
                let ops = diff(&last, &next);
                lead_has = apply(&lead_has, &ops);
                assert_eq!(lead_has, next, "ops {ops:?}");
                last = next;
            }
        }
    }

    #[test]
    fn an_unchanged_block_has_no_changes_and_a_changed_line_is_said() {
        let a = l(&[("h|needs", "Needs routing:"), ("it|1", "- #1 bug: A | new"), ("h|open", "Other open items:"), ("it|2", "- #2 bug: B | queued")]);
        assert!(diff(&a, &a).is_empty());
        let mut b = a.clone();
        b[3].1 = "- #2 bug: B | working".into();
        b.push(("s|x".into(), "- web (sid:x, idle)".into()));
        let ops = diff(&a, &b);
        let said = changes(&a, &b, &ops);
        assert!(said.contains("Other open items:\n- #2 bug: B | working\n"), "{said}");
        assert!(!said.contains("#1"), "{said}");
        let c: Lines = b.iter().filter(|(k, _)| k != "it|1").cloned().collect();
        let said = changes(&b, &c, &diff(&b, &c));
        assert!(said.contains("No longer listed: #1 (done, dropped or moved)"), "{said}");
        assert_eq!(changes(&c, &c, &[]), "Nothing changed.\n");
    }

    #[test]
    fn the_whole_block_and_rules_go_first_after_a_compaction_and_every_twenty() {
        assert_eq!(kind_for(None, "a", 0), Kind::First);
        let mut s = Sent { sid: "a".into(), base: l(&[("it|1", "x")]), since_full: 0, rules_at: 100, ..Default::default() };
        assert_eq!(kind_for(Some(&s), "a", 50), Kind::Changes);
        assert_eq!(kind_for(Some(&s), "b", 50), Kind::First, "a new lead");
        assert_eq!(kind_for(Some(&s), "a", 150), Kind::First, "compacted since");
        s.since_full = FULL_EVERY - 1;
        assert_eq!(kind_for(Some(&s), "a", 50), Kind::Whole);
        s.since_full = 0;
        s.full_next = true;
        assert_eq!(kind_for(Some(&s), "a", 50), Kind::Whole);
    }
}
