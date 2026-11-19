//! Steps held while you're away: on a Desk with Away on, a session's Ask me before step (a delete, a push, a release)
//! doesn't stop it on a card. It's noted here and the session is told it's held and to carry on; the Desk lists each
//! held step under Waiting on you with Allow and No. Once you've answered a session's oldest ones, it's sent them in
//! the order it asked: what you allowed to run exactly as it wrote it, what you said no to. The run that follows is
//! let through once (its own command, by the same session); nothing held runs before that.
//! Kept in config.json (`desk.held`: [{ id, desk, sid, tool, command, input, kinds, at, answer, sent }]), so a restart
//! keeps them.

use super::*;

const KEY: &str = "desk.held";

/// What the session is told when its step is held.
pub(crate) const HELD_WORDS: &str = "Held for the owner while they're away (Ask me before): Cue lists this step for them. You'll be told once they answer; if they allow it, run exactly the same command then. Don't get round it or try another way. Carry on with the rest of your work.";

fn all() -> Vec<Value> {
    crate::config::load().pointer("/desk/held").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn update<R>(f: impl FnOnce(&mut Vec<Value>) -> R) -> Result<R, String> {
    crate::config::update_with(KEY, |c| {
        let mut list = c.pointer("/desk/held").and_then(Value::as_array).cloned().unwrap_or_default();
        let r = f(&mut list);
        Ok((Some(Value::Array(list)), r))
    })
}

/// The words of a tool call that say what it does: a shell command's text, else the whole input.
fn command_of(tool_input: &Value) -> String {
    tool_input.get("command").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| tool_input.to_string())
}

fn str_of<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// What a held step does, in plain words.
fn plain(kinds: &[String]) -> String {
    let mut out: Vec<&str> = vec![];
    for k in kinds {
        let w = match k.as_str() {
            "data" => "delete files",
            "main" | "push" => "push",
            "release" => "release",
            _ => continue,
        };
        if !out.contains(&w) {
            out.push(w);
        }
    }
    if out.is_empty() {
        return "run a step you asked to approve".into();
    }
    if out.len() == 1 {
        return out[0].to_string();
    }
    format!("{} and {}", out[..out.len() - 1].join(", "), out[out.len() - 1])
}

/// A reply already given: the guard's connection gets it at once.
fn given(behavior: &str, message: Option<&str>) -> oneshot::Receiver<Reply> {
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(Reply::Decision(Decision { behavior: behavior.into(), message: message.map(str::to_string), ..Default::default() }));
    rx
}

/// The Desk session `sid` is on whose Away is in force now, if any.
fn away_desk(st: &Store, sid: &str, now: u64) -> Option<String> {
    let desks: Vec<String> = st.desk.lead.keys().cloned().collect();
    desks.into_iter().find(|d| super::desk_away::away_in_force(st, d, now) && super::desk_away::on_desk(st, d, sid))
}

/// `state.desk.projects[i].held`: Desk `desk`'s steps not answered yet, oldest first.
pub(super) fn held_view(st: &Store, desk: &str) -> Value {
    let list: Vec<Value> = all()
        .into_iter()
        .filter(|h| str_of(h, "desk") == desk && str_of(h, "answer").is_empty())
        .map(|h| {
            let sid = str_of(&h, "sid");
            let who = st.sessions.get(sid).map(super::desk::shown_name).unwrap_or_default();
            let kinds: Vec<String> = h.get("kinds").and_then(Value::as_array).map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect()).unwrap_or_default();
            json!({ "id": h["id"], "sid": sid, "who": who, "what": plain(&kinds), "command": h["command"], "at": h["at"] })
        })
        .collect();
    Value::Array(list)
}

impl Hub {
    /// Ask me before's step from `origin`, held instead of asked: let through when you allowed this very command
    /// (once), held when its Desk is away. None: asked as usual on a card.
    pub(super) fn held_guard(&self, origin: &Origin, tool_name: &str, tool_input: &Value, kinds: &[String]) -> Option<(String, oneshot::Receiver<Reply>)> {
        let sid = origin.session_id.clone();
        let command = command_of(tool_input);
        // You allowed it and the session was told to run it: this run is it.
        let allowed = update(|list| {
            let pos = list.iter().position(|h| str_of(h, "sid") == sid && str_of(h, "command") == command && str_of(h, "answer") == "allow" && h.get("sent").and_then(Value::as_bool).unwrap_or(false))?;
            Some(list.remove(pos))
        });
        match allowed {
            Ok(Some(h)) => {
                self.changed();
                return Some((str_of(&h, "id").to_string(), given("allow", None)));
            }
            Ok(None) => {}
            Err(e) => eprintln!("cue: couldn't read the held steps: {e}"),
        }
        let now = now_ms();
        let desk = self.read(|st| away_desk(st, &sid, now))?;
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = format!("held-{now}-{}", N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let entry = json!({ "id": id, "desk": desk, "sid": sid, "tool": tool_name, "command": command, "input": tool_input, "kinds": kinds, "at": now, "answer": "", "sent": false });
        let held = update(|list| {
            // Asked again before you answered: the one already held stands.
            if let Some(h) = list.iter().find(|h| str_of(h, "sid") == sid && str_of(h, "command") == command && str_of(h, "answer").is_empty()) {
                return str_of(h, "id").to_string();
            }
            list.push(entry);
            id.clone()
        });
        let id = match held {
            Ok(id) => id,
            // Not saved: the usual card, so it isn't lost.
            Err(e) => {
                eprintln!("cue: couldn't hold a step: {e}");
                return None;
            }
        };
        self.changed();
        Some((id, given("deny", Some(HELD_WORDS))))
    }

    /// Your answer to held step `id`: `allow` or not. Then each session whose oldest held steps are all answered is
    /// sent them, in the order it asked.
    pub(crate) fn desk_held_answer(&self, id: &str, allow: bool) -> Result<(), String> {
        let found = update(|list| {
            let h = list.iter_mut().find(|h| str_of(h, "id") == id && str_of(h, "answer").is_empty())?;
            h["answer"] = json!(if allow { "allow" } else { "no" });
            Some(str_of(h, "sid").to_string())
        })?;
        let sid = found.ok_or("that step was already answered or isn't held any more")?;
        self.held_release(&sid);
        self.changed();
        Ok(())
    }

    /// Session `sid`'s oldest held steps, as long as each is answered: sent to it in one message, in order. What you
    /// allowed stays as a one-time pass for its run; what you said no to is dropped.
    fn held_release(&self, sid: &str) {
        let ready = update(|list| {
            let mut ready: Vec<Value> = vec![];
            for h in list.iter_mut().filter(|h| str_of(h, "sid") == sid && !h.get("sent").and_then(Value::as_bool).unwrap_or(false)) {
                if str_of(h, "answer").is_empty() {
                    break;
                }
                h["sent"] = json!(true);
                ready.push(h.clone());
            }
            list.retain(|h| !(str_of(h, "sid") == sid && str_of(h, "answer") == "no" && h.get("sent").and_then(Value::as_bool).unwrap_or(false)));
            ready
        });
        let ready = match ready {
            Ok(r) if !r.is_empty() => r,
            Ok(_) => return,
            Err(e) => {
                eprintln!("cue: couldn't release held steps: {e}");
                return;
            }
        };
        let words = release_words(&ready);
        if let Err(e) = self.send_to_session(sid, &words, &[], false) {
            eprintln!("cue: couldn't tell {sid} the owner's answers to its held steps: {e}");
        }
    }
}

/// What a session is told once you've answered its held steps `ready` (oldest first).
fn release_words(ready: &[Value]) -> String {
    let mut out = String::from("The owner answered the steps held while they were away (Ask me before), in the order you asked:\n");
    for (n, h) in ready.iter().enumerate() {
        let verdict = if str_of(h, "answer") == "allow" { "ALLOWED: run it now, exactly as written" } else { "NO: don't run it" };
        out.push_str(&format!("{}. {verdict}:\n{}\n", n + 1, str_of(h, "command")));
    }
    let allowed = ready.iter().filter(|h| str_of(h, "answer") == "allow").count();
    if allowed > 0 {
        out.push_str("Run the allowed ones in this order, one at a time, each only if the one before it worked. If the situation has changed so one no longer makes sense, don't run it: say why.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{plain, release_words};
    use serde_json::json;

    #[test]
    fn held_steps_read_in_plain_words() {
        assert_eq!(plain(&["data".into()]), "delete files");
        assert_eq!(plain(&["main".into(), "push".into()]), "push");
        assert_eq!(plain(&["push".into(), "release".into(), "data".into()]), "push, release and delete files");
    }

    #[test]
    fn the_session_is_told_its_steps_in_order() {
        let w = release_words(&[json!({ "command": "rm -rf build", "answer": "allow" }), json!({ "command": "git push", "answer": "no" })]);
        let (a, b) = (w.find("rm -rf build").unwrap(), w.find("git push").unwrap());
        assert!(a < b && w.contains("1. ALLOWED") && w.contains("2. NO"), "{w}");
    }
}
