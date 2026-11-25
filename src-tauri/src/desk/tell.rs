//! Leads of Desks on the same repository telling each other (#540, docs/desk-coordination-plan.md, option B step 2).
//! `TELL DESK: <Desk> | <words>` goes to that Desk's lead, `TELL DESKS: <words>` to every other Desk lead on the
//! repository, and a HOLD tells them too (desk/holds.rs). Cue finds the Desks itself: the lead needs no lookups. Each
//! goes as Cue's own message to that lead (desk/hub/lead/send.rs `desk_send_cue`: live, kept while it's busy,
//! resumed when parked), headed so its chat draws it as the FROM ANOTHER DESK band (ui/desk/lead.js, #472). Every
//! band sent is kept (`Band`, saved in desk_bands) with whether it was answered, for the question when one goes
//! unanswered (step 5), and noted as a minor decision on both Desks.

use serde::{Deserialize, Serialize};

use crate::desk::holds::repo_key;
use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::turn::route::decide_call;
use crate::desk::parse::Line;
use crate::hub::Store;

/// How long a band is kept: past a day it's neither answered nor asked about.
pub const BAND_KEEP_MS: u64 = 24 * 60 * 60 * 1000;

/// One band sent: Desk `from`'s lead (`by`, its name then; `sid`, its session) told Desk `to`'s lead `text` (its own
/// words), about `item` (an item of `from`'s, when the words start with its #id) at `at` (ms). `ask`: a TELL, which
/// wants an answer (a HOLD's heads-up doesn't); `answered`: when `to`'s lead told `from` back (0: not yet).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Band {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub item: Option<String>,
    pub text: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub sid: String,
    pub at: u64,
    #[serde(default)]
    pub ask: bool,
    #[serde(default)]
    pub answered: u64,
    /// When the user was asked about it (desk/escalate.rs: unanswered, or a lead's DISAGREE with it; 0: never).
    #[serde(default)]
    pub asked: u64,
    /// It answered an open band from `to`: a reply, never asked about as unanswered (desk/escalate.rs).
    #[serde(default)]
    pub reply: bool,
}

impl Band {
    /// Its key where it's saved: one band a sender, receiver and time.
    pub fn key(&self) -> String {
        format!("{}>{}@{}", self.from, self.to, self.at)
    }
}

/// `TELL DESK: <Desk> | <words>` (`to` Some) or `TELL DESKS: <words>` (`to` None), read by parse.rs's `one`. `item`:
/// the #id the words start with, if they do.
pub(crate) fn parse(word: &str, body: &str) -> Option<Line> {
    let (to, words) = match word {
        "TELL DESK" => {
            let (to, words) = body.split_once('|')?;
            let to = to.trim().trim_matches(['`', '"', '*']).trim();
            (Some(to.to_string()).filter(|t| !t.is_empty())?, words.trim())
        }
        "TELL DESKS" => return Some(body.trim()).filter(|w| !w.is_empty()).map(|w| Line::Tell { to: None, item: item_of(w), text: w.to_string() }),
        _ => return None,
    };
    (!words.is_empty()).then(|| Line::Tell { to: Some(to), item: item_of(words), text: words.to_string() })
}

/// The #id words start with ("#533 is half done…"), if they do.
pub(crate) fn item_of(words: &str) -> Option<String> {
    let id: String = words.strip_prefix('#')?.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (!id.is_empty()).then(|| id.to_ascii_lowercase())
}

/// The other Desks with a lead on the repository Desk `desk` works on (desk/holds.rs `repo_key`), by name.
pub(crate) fn others_on_repo(st: &Store, desk: &str) -> Vec<String> {
    let key = repo_key(st, desk);
    let mut out: Vec<String> = st.desk.lead.keys().filter(|d| *d != desk && repo_key(st, d) == key).cloned().collect();
    out.sort_by_key(|d| st.desk.name_of(d).to_lowercase());
    out
}

/// The other Desk on Desk `desk`'s repository that `t` names (its id or name), or why there's none, in words that
/// follow "Refused: <line>: ".
pub(crate) fn desk_named(st: &Store, desk: &str, t: &str) -> Result<String, String> {
    let others = others_on_repo(st, desk);
    let names = others.iter().map(|d| st.desk.name_of(d).to_string()).collect::<Vec<_>>().join(", ");
    if others.is_empty() {
        return Err("no other Desk with a lead is on this repository. You can carry on: nobody else works on this code.".into());
    }
    let named = |d: &str| d == t || st.desk.name_of(d).eq_ignore_ascii_case(t);
    if named(desk) {
        return Err(format!("it's your own Desk. You can tell another Desk on this repository: {names}."));
    }
    others.iter().find(|d| named(d)).cloned().ok_or_else(|| format!("no such Desk on this repository. You can tell one of these: {names}."))
}

/// The lead of Desk `desk` (`by`, session `sid`) said `TELL DESK: <to> | <text>` or `TELL DESKS: <text>`: the band
/// sent, and what the lead is told of it ("Told: loops." or "Refused: …").
#[allow(clippy::too_many_arguments)]
pub(crate) fn tell_said(st: &mut Store, desk: &str, by: &str, sid: &str, to: Option<String>, item: Option<String>, text: &str, now: u64, fx: &mut Effects) -> String {
    let others = others_on_repo(st, desk);
    let line = to.as_deref().map_or("TELL DESKS".to_string(), |t| format!("TELL DESK {t}"));
    if others.is_empty() {
        return format!("Refused: {line}: no other Desk with a lead is on this repository. You can carry on: nobody else works on this code.");
    }
    let to = match to {
        None => others,
        Some(t) => match desk_named(st, desk, &t) {
            Ok(d) => vec![d],
            Err(no) => return format!("Refused: {line}: {no}"),
        },
    };
    let item = item.map(|i| st.desk.canon(&i));
    let told = send(st, desk, by, sid, &to, item, text, true, now, fx);
    format!("Told: {}.", told.join(", "))
}

/// Desk `from`'s lead (`by`, session `sid`) tells the leads of Desks `to` its `words` (about its item `item`): each
/// sent the band, kept, and noted on both Desks. `ask`: a TELL, which is answered, and which answers the open TELLs
/// those Desks sent `from`. The names of the Desks told.
#[allow(clippy::too_many_arguments)]
pub(crate) fn send(st: &mut Store, from: &str, by: &str, sid: &str, to: &[String], item: Option<String>, words: &str, ask: bool, now: u64, fx: &mut Effects) -> Vec<String> {
    let words = words.trim();
    let from_name = st.desk.name_of(from).to_string();
    st.desk.bands.retain(|b| now.saturating_sub(b.at) < BAND_KEEP_MS);
    let mut replied: Vec<String> = Vec::new();
    if ask {
        for b in st.desk.bands.iter_mut().filter(|b| b.ask && b.answered == 0 && b.to == from && to.contains(&b.from)) {
            b.answered = now;
            replied.push(b.from.clone());
            fx.bands.push(b.clone());
        }
    }
    let text = band_text(&from_name, by, item.as_deref(), words);
    let own_item = item.clone().filter(|i| st.desk.item(i).is_some_and(|it| it.project == from));
    let mut told = Vec::new();
    for d in to {
        let mut at = now;
        while st.desk.bands.iter().any(|b| b.from == from && b.to == *d && b.at == at) {
            at += 1;
        }
        let b = Band { from: from.into(), to: d.clone(), item: item.clone(), text: words.into(), by: by.into(), sid: sid.into(), at, ask, answered: 0, asked: 0, reply: replied.contains(d) };
        st.desk.bands.push(b.clone());
        fx.bands.push(b);
        fx.tells.push((d.clone(), text.clone(), sid.to_string(), crate::sessions::Kind::Peer));
        let to_name = st.desk.name_of(d).to_string();
        decide_call(st, from, own_item.clone(), by, sid, (&format!("Told {to_name}'s lead: {words}"), "", true), true, now, fx);
        decide_call(st, d, None, by, sid, (&format!("{from_name}'s lead told this Desk: {words}"), "", true), true, now, fx);
        told.push(to_name);
    }
    told
}

/// The band as the receiving lead gets it (after its Desk's summary). Its chat reads the head back to draw the FROM
/// ANOTHER DESK band (ui/desk/lead.js `DESK_TOLD`): keep the two in step.
pub(crate) fn band_text(from_name: &str, by: &str, item: Option<&str>, words: &str) -> String {
    let about = item.map(|i| format!(" · about #{i}")).unwrap_or_default();
    format!("[From another Desk: {from_name} · lead {by}{about}]\n{}\n\n(To answer: TELL DESK: {from_name} | <your words>.)", words.trim())
}
