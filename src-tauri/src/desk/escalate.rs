//! The user asked when Desks on the same code can't settle it between their leads (#540,
//! docs/desk-coordination-plan.md, option B step 5, "When you hear"). Only three cases reach Waiting on you:
//!
//! 1. A lead writes `DISAGREE: <Desk> | <its side>`: its side, and the other Desk's last band to it as the other.
//! 2. A band that asks (a TELL DESK or TELL DESKS, not one that answers another) has had no TELL back for 15 minutes.
//! 3. A hold has stood more than an hour past its own till, when the till reads as a time ("14:30", "2 hours", an
//!    ISO time; "it lands" never overstays).
//!
//! Each is one question (`ToUser`), put in Waiting on you on every Desk it's about: one card a Desk, through the
//! lead's ASK card (`desk_ask`), linked by `key`. Answering one closes the others, sends the answer to each of those
//! leads, and notes it on each Desk's decisions. Each case is asked once: the band or hold keeps when it was asked
//! (`asked`, saved with it).

use serde::{Deserialize, Serialize};

use crate::desk::holds::{Hold, HoldSave};
use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::prelude::*;
use crate::desk::hub::turn::route::decide_call;
use crate::desk::parse::Line;
use crate::desk::tell::Band;

/// A band that asks, unanswered this long, is brought to the user.
pub const UNANSWERED_MS: u64 = 15 * 60 * 1000;
/// A hold standing this long past its till is brought to the user.
pub const OVERSTAY_MS: u64 = 60 * 60 * 1000;

/// The answer to an overstayed hold's question that lifts it.
pub const LIFT: &str = "Lift the hold";

/// One question to the user about Desks on the same code: `kind` "disagree", "unanswered" or "overstay"; `key` links
/// its cards (one a Desk in `desks`); `hold`: the item whose hold it's about.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ToUser {
    pub kind: String,
    pub key: String,
    pub desks: Vec<String>,
    pub question: String,
    pub choices: Vec<String>,
    #[serde(default)]
    pub hold: Option<String>,
}

/// `DISAGREE: <Desk> | <words>`, read by parse.rs's `one`; `item`: the #id the words start with.
pub(crate) fn parse(body: &str) -> Option<Line> {
    let (to, words) = body.split_once('|')?;
    let to = to.trim().trim_matches(['`', '"', '*']).trim();
    let words = words.trim();
    (!to.is_empty() && !words.is_empty()).then(|| Line::Disagree { to: to.to_string(), item: crate::desk::tell::item_of(words), text: words.to_string() })
}

/// The lead of Desk `desk` (`by`, session `sid`) said `DISAGREE: <to> | <text>`: the question to the user, with the
/// other Desk's last band to this one as its side, and that lead told. What the lead is told of it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn disagree_said(st: &mut Store, desk: &str, by: &str, sid: &str, to: &str, item: Option<String>, text: &str, now: u64, fx: &mut Effects) -> String {
    let line = format!("DISAGREE {to}");
    let other = match crate::desk::tell::desk_named(st, desk, to) {
        Ok(d) => d,
        Err(no) => return format!("Refused: {line}: {no}"),
    };
    let (mine, theirs) = (st.desk.name_of(desk).to_string(), st.desk.name_of(&other).to_string());
    let Some(band) = st.desk.bands.iter_mut().rev().find(|b| b.from == other && b.to == desk) else {
        return format!("Refused: {line}: {theirs}'s lead hasn't told this Desk anything. You can say your side first with TELL DESK: {theirs} | <your words>; ask the user only when you can't agree.");
    };
    if band.asked != 0 {
        return format!("Refused: {line}: the user is asked already. Their answer comes to you and {theirs}'s lead.");
    }
    band.asked = now;
    // It's this lead's answer to that band, too: never asked about again as unanswered.
    if band.answered == 0 {
        band.answered = now;
    }
    let band = band.clone();
    fx.bands.push(band.clone());
    let item = item.map(|i| st.desk.canon(&i));
    let choices = match (band.item.as_deref(), item.as_deref()) {
        (Some(a), Some(b)) if a != b => vec![format!("#{a} goes first, #{b} waits"), format!("#{b} goes first, #{a} waits")],
        _ => vec![format!("Go with {theirs}'s lead"), format!("Go with {mine}'s lead")],
    };
    let question = format!("The leads of {theirs} and {mine} can't agree on shared code. {theirs}'s lead: \"{}\" {mine}'s lead: \"{}\" Which way?", band.text.trim(), text.trim());
    fx.to_user.push(ToUser { kind: "disagree".into(), key: format!("disagree:{}", band.key()), desks: vec![other.clone(), desk.to_string()], question, choices, hold: None });
    crate::desk::tell::send(st, desk, by, sid, std::slice::from_ref(&other), item, &format!("I don't agree: {}. Cue asks the user to choose.", text.trim().trim_end_matches('.')), false, now, fx);
    // "Asked:" as an ASK's: not sent back (the card is in this lead's name, and it hears the answer).
    format!("Asked: the user, on both Desks. Their answer comes to you and {theirs}'s lead.")
}

/// The questions now due on the Desks: bands that ask, unanswered 15 minutes, and holds an hour past their till; each
/// marked asked (saved with `fx`) and put in `fx.to_user`.
pub(crate) fn due(st: &mut Store, now: u64, fx: &mut Effects) {
    let led = |st: &Store, d: &str| st.desk.lead_sid(d).is_some();
    let late: Vec<usize> = (0..st.desk.bands.len()).filter(|&i| {
        let b = &st.desk.bands[i];
        b.ask && !b.reply && b.answered == 0 && b.asked == 0 && now.saturating_sub(b.at) >= UNANSWERED_MS && led(st, &b.from) && led(st, &b.to)
    }).collect();
    for i in late {
        st.desk.bands[i].asked = now;
        let b = st.desk.bands[i].clone();
        fx.bands.push(b.clone());
        fx.to_user.push(unanswered(st, &b));
    }
    let over: Vec<Hold> = st.desk.holds.iter().filter(|h| {
        h.asked == 0 && crate::desk::holds::standing(st, h) && till_at(&h.till, h.at).is_some_and(|t| now >= t + OVERSTAY_MS)
    }).cloned().collect();
    for mut h in over {
        h.asked = now;
        if let Some(x) = st.desk.holds.iter_mut().find(|x| x.item == h.item) {
            x.asked = now;
        }
        fx.holds.push(HoldSave::Put(h.clone()));
        let mut desks = vec![h.desk.clone()];
        desks.extend(crate::desk::tell::others_on_repo(st, &h.desk));
        let till = h.till.trim();
        // "an hour past 14:30", but "an hour past its till of 2 hours".
        let past = if relative(&stripped(till)).is_some() { format!("its till of {}", stripped(till)) } else { till.to_string() };
        let question = format!("{} #{} still holds {}, an hour past {past}.", st.desk.name_of(&h.desk), h.item, h.area.join(", "));
        fx.to_user.push(ToUser { kind: "overstay".into(), key: format!("overstay:{}@{}", h.item, h.at), desks, question, choices: vec![LIFT.into(), "Keep it".into()], hold: Some(h.item.clone()) });
    }
}

/// The question on band `b`, unanswered 15 minutes.
fn unanswered(st: &Store, b: &Band) -> ToUser {
    let (from, to) = (st.desk.name_of(&b.from).to_string(), st.desk.name_of(&b.to).to_string());
    let question = format!("{from}'s lead asked {to}'s lead: \"{}\" No answer from {to}'s lead in 15 min.", b.text.trim());
    ToUser { kind: "unanswered".into(), key: format!("unanswered:{}", b.key()), desks: vec![b.from.clone(), b.to.clone()], question, choices: vec![format!("Tell {to}'s lead to answer"), "Go ahead without them".into()], hold: None }
}

/// Whether any question is due now, read only (the tick's cheap look before it writes).
pub(crate) fn any_due(st: &Store, now: u64) -> bool {
    st.desk.bands.iter().any(|b| b.ask && !b.reply && b.answered == 0 && b.asked == 0 && now.saturating_sub(b.at) >= UNANSWERED_MS)
        || st.desk.holds.iter().any(|h| h.asked == 0 && till_at(&h.till, h.at).is_some_and(|t| now >= t + OVERSTAY_MS))
}

/// When a hold's till ends (ms), if its words read as a time: "14:30", "2:30pm", "5pm" (the next such time from
/// `at`, local), "2 hours", "in 30 min", "90m" (from `at`), or an ISO time ("2026-11-21T14:30", with "Z" or an
/// offset, else local). None for anything else ("it lands", "tomorrow's release").
pub(crate) fn till_at(till: &str, at: u64) -> Option<u64> {
    let t = stripped(till);
    let t = t.as_str();
    relative(t).map(|ms| at + ms).or_else(|| clock(t, at)).or_else(|| iso(t, at))
}

/// A till's words, lower case, without "in", "about", "for" or "~" first.
fn stripped(till: &str) -> String {
    let t = till.trim().to_lowercase();
    ["in ", "about ", "for ", "~"].iter().fold(t.as_str(), |t, p| t.strip_prefix(p).unwrap_or(t).trim()).to_string()
}

/// "2 hours", "30 min", "an hour", "90m": how long, in ms.
fn relative(t: &str) -> Option<u64> {
    let t = t.strip_prefix("an ").or_else(|| t.strip_prefix("a ")).map(|r| format!("1 {r}")).unwrap_or_else(|| t.to_string());
    let n: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let unit = t[n.len()..].trim().trim_end_matches('.');
    let n: u64 = n.parse().ok()?;
    let each = match unit {
        "m" | "min" | "mins" | "minute" | "minutes" => 60_000,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000,
        "d" | "day" | "days" => 86_400_000,
        _ => return None,
    };
    Some(n * each)
}

/// Local seconds east of UTC at `at` (ms).
fn offset_s(at: u64) -> i64 {
    crate::logs::usage::local_tm(at / 1000).tm_gmtoff as i64
}

/// "14:30", "2:30pm", "5pm", "5 pm": the first such local time at or after `at` (ms).
fn clock(t: &str, at: u64) -> Option<u64> {
    let (t, pm) = match t.strip_suffix("pm").or_else(|| t.strip_suffix("am")) {
        Some(r) => (r.trim(), Some(t.ends_with("pm"))),
        None => (t, None),
    };
    let (h, m) = match t.split_once(':') {
        Some((h, m)) => (h.parse::<u64>().ok()?, m.parse::<u64>().ok()?),
        None if pm.is_some() => (t.parse::<u64>().ok()?, 0),
        None => return None,
    };
    let h = match pm {
        Some(true) if (1..12).contains(&h) => h + 12,
        Some(false) if h == 12 => 0,
        Some(_) if !(1..=12).contains(&h) => return None,
        _ => h,
    };
    if h > 23 || m > 59 {
        return None;
    }
    let day = 86_400_000i64;
    let local = at as i64 + offset_s(at) * 1000;
    let mut want = local - local.rem_euclid(day) + (h * 3_600_000 + m * 60_000) as i64;
    if want < local {
        want += day;
    }
    u64::try_from(want - offset_s(at) * 1000).ok()
}

/// "2026-11-21T14:30[:00][Z|+01:00]" (or a space for the T): its time in ms; local when it names no offset.
fn iso(t: &str, at: u64) -> Option<u64> {
    let (date, rest) = t.split_once(['t', ' '])?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let (time, off) = if let Some(r) = rest.strip_suffix('z') {
        (r, Some(0))
    } else if let Some(i) = rest.rfind(['+', '-']) {
        let (sign, o) = (if rest[i..].starts_with('-') { -1 } else { 1 }, &rest[i + 1..]);
        let (oh, om) = o.split_once(':').unwrap_or((o, "0"));
        (&rest[..i], Some(sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60)))
    } else {
        (rest, None)
    };
    let mut p = time.split(':').map(|x| x.split('.').next().unwrap_or("").parse::<i64>().ok());
    let (h, mi, s) = (p.next()??, p.next()??, p.next().flatten().unwrap_or(0));
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || h > 23 || mi > 59 {
        return None;
    }
    // Days from 1970-01-01 to the date (Howard Hinnant's days_from_civil).
    let (yy, mm) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + da - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    let secs = days * 86_400 + h * 3600 + mi * 60 + s - off.unwrap_or_else(|| offset_s(at));
    u64::try_from(secs * 1000).ok()
}

impl Hub {
    /// The questions due now on the Desks (`due`), from the Desk's tick. Only read unless there's one.
    pub(crate) fn desk_to_user_tick(&self) {
        let now = now_ms();
        if !self.read(|st| any_due(st, now)) {
            return;
        }
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            due(st, now, &mut fx);
            fx
        });
        self.desk_done(fx);
    }

    /// Question `q` put in Waiting on you on each of its Desks with a lead: one card a Desk, linked by its key.
    pub(crate) fn desk_to_user_ask(&self, q: &ToUser) {
        for d in &q.desks {
            let Some(lead) = self.read(|st| st.desk.lead_sid(d).map(str::to_string)) else { continue };
            let Some(id) = self.desk_ask(&lead, d, &q.question, &q.choices, None, None) else { continue };
            self.write(|st| {
                if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                    it.tool_input["desk_to_user"] = json!(q);
                }
            });
        }
    }

    /// Card `id` of question `q` answered with `answer`: its cards on the other Desks closed with it, the hold lifted
    /// on "Lift the hold", and the answer noted on each Desk and sent to each lead.
    pub(crate) fn desk_to_user_answered(&self, id: &str, q: ToUser, answer: &str) {
        let others: Vec<String> = self.read(|st| st.items.iter().filter(|i| i.id != id && i.tool_input["desk_to_user"]["key"].as_str() == Some(q.key.as_str())).map(|i| i.id.clone()).collect());
        for o in others {
            self.write(|st| st.waiters.remove(&o));
            self.finish(&o, "answered", &format!("answered on another Desk: {answer}"));
        }
        let now = now_ms();
        let lift = q.kind == "overstay" && answer == LIFT;
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            if let Some(item) = q.hold.as_ref().filter(|_| lift) {
                if st.desk.holds.iter().any(|h| h.item == *item) {
                    st.desk.holds.retain(|h| h.item != *item);
                    fx.holds.push(HoldSave::Gone(item.clone()));
                    fx.land = true;
                }
            }
            for d in &q.desks {
                let item = q.hold.clone().filter(|i| st.desk.item(i).is_some_and(|x| x.project == *d));
                decide_call(st, d, item, "You", "", (&format!("You answered: {answer}"), &q.question, false), false, now, &mut fx);
            }
            fx
        });
        self.desk_done(fx);
        let names = self.read(|st| q.desks.iter().map(|d| st.desk.name_of(d).to_string()).collect::<Vec<_>>().join(" and "));
        let lifted = if lift { " The hold is lifted." } else { "" };
        let words = format!("Cue asked the user, for the leads of {names}: {}\nThe user's answer: {answer}.{lifted} Go by it.", q.question);
        for d in q.desks {
            let words = words.clone();
            self.later(0, move |h| {
                if let Err(e) = crate::desk::hub::lead::desk_send_cue(h, &d, &words, crate::sessions::Kind::Question) {
                    eprintln!("cue: desk: the user's answer didn't reach {d}'s lead: {e}");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_till_reads_as_a_time_only_when_it_is_one() {
        let at = 1_000_000_000_000;
        assert_eq!(till_at("2 hours", at), Some(at + 7_200_000));
        assert_eq!(till_at("in 30 min", at), Some(at + 1_800_000));
        assert_eq!(till_at("about an hour", at), Some(at + 3_600_000));
        assert_eq!(till_at("90m", at), Some(at + 5_400_000));
        assert_eq!(till_at("2026-11-21T14:30Z", at), Some(1_795_271_400_000));
        assert_eq!(till_at("2026-11-21T15:30+01:00", at), Some(1_795_271_400_000));
        for words in ["it lands", "", "tomorrow's release", "25:00", "soon"] {
            assert_eq!(till_at(words, at), None, "{words}");
        }
        let at_clock = till_at("14:30", at).unwrap();
        assert!(at_clock >= at && at_clock < at + 86_400_000);
        assert_eq!(till_at("2:30pm", at), Some(at_clock));
    }
}
