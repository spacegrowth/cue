//! You picking an option on a Desk item (#281, #543): which option your words name, the options a pick line offers,
//! and what follows the pick (its follow-up items, filed and routed).

use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;
use crate::desk::hub::core::desk::{Effects, ask_of, secs};
use crate::desk::hub::turn::act::you_wrote;
use crate::desk::hub::turn::route::release;
use crate::desk::hub::turn::heard::new_item;

/// The option you picked ("B", "option b" → "B"), or "" when there's none.
fn option_of(text: &str) -> String {
    if let Some(o) = letter_of(text.trim().trim_end_matches('.')) {
        return o;
    }
    let t = text.trim().trim_end_matches('.').trim();
    let t = t.strip_prefix("option ").or_else(|| t.strip_prefix("Option ")).unwrap_or(t).trim();
    if t.chars().count() == 1 { t.to_uppercase() } else { t.chars().take(40).collect::<String>().trim().to_string() }
}

/// What an item is about, from its title: its first words before a colon go ("Mocks: working items", "Plan: the
/// landing line" → "working items", "the landing line"), as does a closing "(…)" note.
fn topic_of(title: &str) -> String {
    let t = title.trim();
    let mut topic = t;
    if let Some((head, rest)) = t.split_once(':') {
        if head.split_whitespace().count() <= 2 && !rest.trim().is_empty() {
            topic = rest.trim();
        }
    }
    if topic.ends_with(')') {
        if let Some(at) = topic.rfind(" (") {
            topic = topic[..at].trim();
        }
    }
    if topic.is_empty() { t.to_string() } else { topic.to_string() }
}

/// The option an answer names, as one capital letter, a letter and a number (M1, #445), or a number of one or two
/// digits (mocks 1 to 10, #619): "B", "b", "Option B", "B: the compact list", "B) …", "M2", "9", "Option 10", "4: …".
/// None for anything else ("A new tab" isn't option A, nor "10 things" option 10).
pub(crate) fn letter_of(answer: &str) -> Option<String> {
    let a = answer.trim();
    let a = if a.len() > 7 && a[..7].eq_ignore_ascii_case("option ") { a[7..].trim_start() } else { a };
    let marks = [':', ')', '.', '-', '\u{2014}', '\u{2013}', ',', '('];
    let number: String = a.chars().take_while(char::is_ascii_digit).collect();
    if !number.is_empty() {
        let rest = a[number.len()..].trim_start();
        return (number.len() <= 2 && (rest.is_empty() || rest.starts_with(marks))).then_some(number);
    }
    let mut chars = a.chars();
    let c = chars.next().filter(char::is_ascii_alphabetic)?;
    let digits: String = chars.as_str().chars().take_while(char::is_ascii_digit).take(2).collect();
    let rest = chars.as_str()[digits.len()..].trim_start();
    if rest.starts_with(|d: char| d.is_ascii_digit()) {
        return None;
    }
    let single = rest.is_empty();
    let marked = c.is_ascii_uppercase() && rest.starts_with(marks);
    (single || marked).then(|| format!("{}{digits}", c.to_ascii_uppercase()))
}

/// The options a pick line names ("Pick one for working items: A, B or C." gives A, B, C): its newest words' line
/// with "pick" or "choose" in it (the one starting so first), and the first run of two or more single capital letters
/// (or numbers, #619: "4, 6, 8 or 9") joined by ",", "/", "or" or "and" in it, else a range ("1 to 10", "A–F").
/// As the Desk page reads it (deskPickOptions).
pub(crate) fn pick_letters(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').flat_map(|l| l.split_inclusive(['.', '?', '!'])).map(str::trim).collect();
    let words = |l: &str| l.split(|c: char| !c.is_alphanumeric()).map(str::to_ascii_lowercase).collect::<Vec<_>>();
    let asks: Vec<&str> = lines.into_iter().filter(|l| words(l).iter().any(|w| w == "pick" || w == "choose")).collect();
    let starts = |l: &str| words(l).first().is_some_and(|w| matches!(w.as_str(), "pick" | "choose" | "which"));
    let Some(line) = asks.iter().find(|l| starts(l)).or(asks.first()) else { return vec![] };
    // Words, with "," and "/" as their own; anything else breaks a run.
    let mut toks: Vec<String> = vec![];
    let mut word = String::new();
    for c in line.chars().chain([' ']) {
        if c.is_alphanumeric() {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            toks.push(std::mem::take(&mut word));
        }
        if !c.is_whitespace() {
            toks.push(match c {
                ',' | '/' => c.to_string(),
                '-' | '\u{2013}' | '\u{2014}' => "to".into(),
                _ => "|".into(),
            });
        }
    }
    // A, B… or M1, M2… (#445), or 1, 2… (#619).
    let number = |t: &str| (1..=2).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit());
    let letter = |t: &str| number(t) || (1..=3).contains(&t.len()) && t.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && t.chars().skip(1).all(|c| c.is_ascii_digit());
    let joins = |t: &str| matches!(t, "," | "/" | "or" | "and");
    for start in 0..toks.len() {
        if !letter(&toks[start]) {
            continue;
        }
        let (mut run, mut i) = (vec![toks[start].clone()], start + 1);
        loop {
            let mut j = i;
            while j < toks.len() && joins(&toks[j]) {
                j += 1;
            }
            if j == i || j >= toks.len() || !letter(&toks[j]) {
                break;
            }
            run.push(toks[j].clone());
            i = j + 1;
        }
        if run.len() > 1 {
            run.dedup();
            return run;
        }
    }
    (0..toks.len()).find_map(|s| toks.get(s..s + 3).and_then(|r| range(&r[0], &r[1], &r[2]))).unwrap_or_default()
}

/// "1 to 10" or "A–F" (its dash read as "to"): each option from `from` to `to`, two to twenty of them; None for
/// anything else.
fn range(from: &str, to_word: &str, to: &str) -> Option<Vec<String>> {
    if to_word != "to" {
        return None;
    }
    let (a, b) = match (from.parse::<u32>(), to.parse::<u32>()) {
        (Ok(a), Ok(b)) if from.len() <= 2 && to.len() <= 2 => (a, b),
        _ if from.len() == 1 && to.len() == 1 && from.chars().chain(to.chars()).all(|c| c.is_ascii_uppercase()) => (from.as_bytes()[0] as u32, to.as_bytes()[0] as u32),
        _ => return None,
    };
    let letters = from.as_bytes()[0].is_ascii_uppercase();
    (b > a && b - a < 20).then(|| (a..=b).map(|n| if letters { char::from_u32(n).unwrap_or('?').to_string() } else { n.to_string() }).collect())
}

/// You picked option `text` on item `id` (#281, #543): it's done, picked. The follow-ups its holder named for that
/// option (IF PICKED: lines, `DeskItem::next`) are filed, each linked back to it (`builds`), and routed at once to
/// whom each names: its holder ("me"), a new session, or a session by name; one Cue can't route goes to the lead.
/// Named none: an item with a canvas gets one "Build: <topic> (option X)" item for the lead to route; any other just
/// closes. Picked already: nothing new, and the answer says what it's being built as.
pub(crate) fn pick(st: &mut Store, id: &str, text: &str, now: u64) -> Result<Effects, String> {
    let mut fx = Effects::default();
    let id = st.desk.canon(id);
    let it = st.desk.item(&id).ok_or("no such item")?.clone();
    let option = pickable(st, &it, text)?;
    let canvas = canvas_of(&it);
    let filed = file_follow_ups(st, &it, &option, &canvas, now);
    let ids: Vec<String> = filed.iter().map(|(n, _)| n.clone()).collect();
    fx.answers.extend(question_answered(st, &it, &option));
    // Done first, so its holder is free to take what follows.
    let m = st.desk.item_mut(&id).ok_or("no such item")?;
    (m.picked, m.built_as) = (option.clone(), ids.first().cloned().unwrap_or_default());
    close_picked(st, &it, &option, &ids, now, &mut fx);
    let (routed, heard) = route_follow_ups(st, &it, &option, &filed, now, &mut fx);
    // The lead hears every pick (#667), so it doesn't close the item again or wait on it, and its chat shows the one
    // line ("You picked B2 on #652; #666 builds it"). Something of it left for it to route: at once, so it routes it
    // now, ahead of the new item's row. All of it routed already: no lead step, the line goes with its next message.
    let route = if heard.is_empty() { String::new() } else { format!(" {}.", heard.join("; ")) };
    let words = format!("The owner picked option {option} on #{id} ({}): \"{}\". #{id} is done; {}.{route}", it.title, text.trim(), follows_words(&ids));
    let first = ids.first().cloned().unwrap_or_default();
    if !heard.is_empty() {
        crate::desk::hub::lead::feed::raise(st, &it.project, &id, crate::desk::feed::ChangeKind::Picked, words, &format!("{option}:{first}"), now);
    } else if st.desk.lead_sid(&it.project).is_some() {
        crate::desk::feed::add(&mut st.desk.feed, &it.project, &id, crate::desk::feed::ChangeKind::YouDid, words, &now.to_string(), now);
    }
    tell_holder(&it, &option, &ids, &routed, &canvas, &mut fx);
    Ok(fx)
}

/// The lead filed item `build` for your pick of `option` on item `id` in its chat (`ITEM: … | from: #<id> | picked:
/// <option>`, #606, #667): `id` closes at once as a Pick on its page closes it, even while its holder is still at
/// work or finishing on it; its holder is told, and a question it waits on is answered. One done or dropped already
/// stays as it is.
pub(crate) fn picked_in_chat(st: &mut Store, id: &str, build: &str, option: &str, now: u64, fx: &mut Effects) {
    let Some(it) = st.desk.item(id).filter(|i| !matches!(i.state.as_str(), "done" | "dropped")).cloned() else { return };
    let ids = vec![build.to_string()];
    fx.answers.extend(question_answered(st, &it, option));
    close_picked(st, &it, option, &ids, now, fx);
    let routed = vec![format!("#{build}, which the Desk lead filed")];
    tell_holder(&it, option, &ids, &routed, &canvas_of(&it), fx);
}

/// Item `it` done by your pick of `option` (picked and built_as set already), `follows` filed after it: done whatever
/// stage it was at, as its holder may still be at work or finishing on it (#667: nothing later takes a done item back
/// to check), and what waited on it let go.
fn close_picked(st: &mut Store, it: &DeskItem, option: &str, follows: &[String], now: u64, fx: &mut Effects) {
    let id = &it.id;
    if let Some(m) = st.desk.item_mut(id) {
        (m.state, m.after, m.close_on_check, m.done_at, m.updated) = ("done".into(), None, false, Some(secs(now)), secs(now));
        (m.paused, m.asks) = (None, None);
    }
    fx.items.push(id.clone());
    you_wrote(st, &it.project, id, &format!("Picked option {option}"), &follows_words(follows), now, fx);
    release(st, id, now, fx);
}

/// What follows a pick, in a few words: "#666 builds it", "#666, #667 follow it", "nothing follows it".
pub(crate) fn follows_words(ids: &[String]) -> String {
    match ids {
        [] => "nothing follows it".to_string(),
        [one] => format!("#{one} builds it"),
        many => format!("{} follow it", many.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ")),
    }
}

/// Its holder, if any, told it's done by your pick and what follows, so it doesn't carry on there.
fn tell_holder(it: &DeskItem, option: &str, ids: &[String], routed: &[String], canvas: &str, fx: &mut Effects) {
    let Some(h) = it.holder.as_ref() else { return };
    let id = &it.id;
    let follows = follows_words(ids);
    let follows = format!("{}{}", follows[..1].to_uppercase(), &follows[1..]);
    let next = if routed.is_empty() { String::new() } else { format!(" What follows: {}.", routed.join("; ")) };
    let mark = if canvas.is_empty() { String::new() } else { format!(" Mark \"Picked: {option}\" where it shows its options ({canvas}), keeping the others.") };
    fx.sends.push(crate::desk::hub::core::desk::kinded((h.to().to_string(), format!("Desk item #{id} ({}): option {option} was picked, so it's done. {follows}.{next}{mark} Do no other work on #{id}.", it.title)), crate::sessions::Kind::Note));
}

/// An item of options to pick from (#667): mocks or a canvas, what follows each option (IF PICKED: lines), or a pick
/// made on it already. The lead's DONE or CLOSE on one is never refused because its holder still has it.
pub(crate) fn has_options(it: &DeskItem) -> bool {
    !it.picked.is_empty() || it.mocks_only || !it.designs.trim().is_empty() || !it.next.is_empty() || it.finish.as_ref().is_some_and(|f| f.mocks)
}

/// The option `text` picks on `it`, or why it can't be picked: picked already (and being built as an item still
/// there), dropped, or no option named.
fn pickable(st: &Store, it: &DeskItem, text: &str) -> Result<String, String> {
    if !it.built_as.is_empty() && st.desk.item(&it.built_as).is_some() {
        return Err(format!("Option {} is picked already: it's being built as #{}", it.picked, it.built_as));
    }
    if !it.picked.is_empty() && it.built_as.is_empty() {
        return Err(format!("Option {} is picked already", it.picked));
    }
    if it.state == "dropped" {
        return Err("It was dropped".into());
    }
    let option = option_of(text);
    if option.is_empty() {
        return Err("Say which option you picked".into());
    }
    Ok(option)
}

/// Its canvas: the DESIGNS: link its session gave, or, for an item whose work changed nothing the app runs (#408,
/// #436), the link it made. Empty when it has none.
fn canvas_of(it: &DeskItem) -> String {
    let no_code = it.finish.as_ref().is_some_and(|f| f.mocks);
    if !it.designs.trim().is_empty() {
        it.designs.trim().to_string()
    } else if no_code {
        it.made.iter().find(|m| m.starts_with("http://") || m.starts_with("https://")).cloned().unwrap_or_default()
    } else {
        String::new()
    }
}

/// Files what follows `option` picked on `it`: the follow-ups its holder named, or, with none named and a canvas,
/// one "Build: <topic> (option X)" item. Each: (its id, whom it names to take it; empty for the lead).
fn file_follow_ups(st: &mut Store, it: &DeskItem, option: &str, canvas: &str, now: u64) -> Vec<(String, String)> {
    let id = &it.id;
    let named: Vec<crate::desk::model::FollowUp> = it.next.iter().filter(|n| n.option == "any" || n.option.eq_ignore_ascii_case(option)).cloned().collect();
    let on = if canvas.is_empty() { String::new() } else { format!(": {canvas}") };
    let mut filed: Vec<(String, String)> = Vec::new();
    for n in &named {
        let mut b = new_item(st, &it.project, &n.kind, &n.title, &n.done_when, "you", now);
        b.see = n.see.trim().to_string();
        (b.builds, b.from_pick) = (id.clone(), option.to_string());
        b.why = format!("You picked option {option} on #{id}{on}");
        filed.push((b.id.clone(), n.by.clone()));
        st.desk.items.push(b);
    }
    if named.is_empty() && !canvas.is_empty() {
        let kind = if it.kind.is_empty() { "feature" } else { it.kind.as_str() };
        let title = format!("Build: {} (option {option})", topic_of(&it.title));
        let done_when = format!("option {option} of #{id} is in the app and works as #{id} shows it; where #{id} shows its options ({canvas}), it says \"Picked: {option}\"");
        let mut b = new_item(st, &it.project, kind, &title, &done_when, "you", now);
        (b.builds, b.from_pick) = (id.clone(), option.to_string());
        b.why = format!("You picked option {option} on #{id}{on}");
        filed.push((b.id.clone(), String::new()));
        st.desk.items.push(b);
    }
    filed
}

/// Its holder waiting on its question (one): the question answered with the option picked. An ask of several
/// questions isn't answered for you.
fn question_answered(st: &Store, it: &DeskItem, option: &str) -> Option<(String, String, String)> {
    let one = |a: &&Item| a.tool_input["questions"].as_array().is_some_and(|q| q.len() == 1);
    it.holder.as_ref().and_then(|h| ask_of(st, h)).filter(|a| a.kind == "question").filter(one).map(|a| {
        let q = a.tool_input.pointer("/questions/0");
        let question = q.and_then(|q| q["question"].as_str()).unwrap_or("").to_string();
        let labels = q.and_then(|q| q["options"].as_array()).map(|o| o.iter().filter_map(|x| x["label"].as_str()).collect::<Vec<_>>()).unwrap_or_default();
        let label = labels.into_iter().find(|l| letter_of(l).as_deref() == Some(option)).map_or_else(|| format!("Option {option}"), str::to_string);
        (a.id.clone(), question, label)
    })
}

/// Each filed follow-up routed as it says, with no lead step; one with no taker, or one Cue can't route, to the
/// lead. Returns where each went, in plain lines for its holder, and what's left to the lead, in lines for it.
fn route_follow_ups(st: &mut Store, it: &DeskItem, option: &str, filed: &[(String, String)], now: u64, fx: &mut Effects) -> (Vec<String>, Vec<String>) {
    let id = &it.id;
    let lead = st.desk.lead.get(&it.project).cloned();
    let (lead_name, lead_sid) = (lead.as_ref().map(|l| l.name.clone()).unwrap_or_else(|| "You".into()), st.desk.lead_sid(&it.project).unwrap_or_default().to_string());
    let held = it.holder.as_ref().filter(|h| h.kind != "crew").map(|h| h.name.clone());
    let mut routed: Vec<String> = Vec::new();
    let mut heard: Vec<String> = Vec::new();
    for (n, by) in filed {
        let title = st.desk.item(n).map(|b| b.title.clone()).unwrap_or_default();
        you_wrote(st, &it.project, n, &format!("Follows option {option}, picked on #{id}"), "", now, fx);
        fx.items.push(n.clone());
        let to = if by.eq_ignore_ascii_case("me") { held.clone() } else if by.is_empty() { None } else { Some(by.clone()) };
        let said = to.as_ref().map(|to| {
            let by = crate::desk::hub::turn::route::By { name: &lead_name, sid: &lead_sid };
            let why = format!("you picked option {option} on #{id}, and #{id} named this to follow it");
            crate::desk::hub::turn::route::route(st, &it.project, &by, n, to, None, &why, now, fx)
        });
        let taken = st.desk.item(n).is_some_and(|b| b.holder.is_some() || b.state != "new") || fx.starts.iter().any(|s| s.id == *n);
        let refused = said.flatten().is_some_and(|w| w.starts_with("Refused"));
        if taken && !refused {
            routed.push(format!("#{n} to {}", to.unwrap_or_default()));
        } else if lead_sid.is_empty() {
            routed.push(format!("#{n} waits on the Desk for someone to take it"));
        } else {
            fx.news.push((it.project.clone(), n.clone()));
            heard.push(format!("Cue filed #{n} {title} to follow it: route it now"));
            routed.push(format!("#{n} goes to the Desk lead to route"));
        }
    }
    (routed, heard)
}
