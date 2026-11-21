//! Leads of Desks on the same repository telling each other (#540, docs/desk-coordination-plan.md option B, step 2):
//! TELL DESK and TELL DESKS reach the other leads as the FROM ANOTHER DESK band (#472), with no lookups by the lead;
//! a HOLD tells them too; each band is kept, with whether it was answered, and noted on both Desks.

use super::desk::{desk, ends, item, sent_to};
use crate::desk::parse::{lines, Line};
use crate::testing::World;

/// A new independent Desk named `name` on folder `folder`, with its lead: (Desk, lead).
fn start_in(w: &World, folder: &std::path::Path, name: &str) -> (String, String) {
    let r = crate::ipc::desk_start_named(w.hub(), &folder.to_string_lossy(), true, Some(name)).unwrap();
    (r["id"].as_str().unwrap().into(), r["sid"].as_str().unwrap().into())
}

fn start(w: &World, name: &str) -> (String, String) {
    start_in(w, w.home(), name)
}

/// A Desk on another repository: (Desk, lead).
fn elsewhere(w: &World) -> (String, String) {
    let other = w.home().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    start_in(w, &other, "Elsewhere")
}

/// The bands Cue delivered to lead `sid` (its messages with a band's head).
fn bands_to(w: &World, sid: &str) -> Vec<String> {
    sent_to(w, sid).into_iter().filter(|t| t.contains("[From another Desk: ")).collect()
}

/// Lead `lead` says `line`: the one answer Cue sends it.
fn answer(w: &World, lead: &str, line: &str) -> String {
    let before = sent_to(w, lead).len();
    ends(w, lead, line);
    let sent: Vec<String> = sent_to(w, lead).into_iter().skip(before).filter(|t| !t.contains("[From another Desk: ")).collect();
    assert_eq!(sent.len(), 1, "one answer to {line}: {sent:?}");
    sent[0].clone()
}

/// The decisions on Desk `d`, their words.
fn decisions(w: &World, d: &str) -> Vec<String> {
    desk(w, d)["decisions"].as_array().unwrap().iter().map(|x| x["text"].as_str().unwrap_or("").to_string()).collect()
}

#[test]
fn tell_lines_read_with_the_item_they_start_with() {
    assert_eq!(
        lines("TELL DESK: loops | #533 is half done in ui/desk/thread.js. Land it first?"),
        [Line::Tell { to: Some("loops".into()), item: Some("533".into()), text: "#533 is half done in ui/desk/thread.js. Land it first?".into() }]
    );
    assert_eq!(lines("- **TELL DESKS: #509 moves hub/desk_*.rs for 2 hours | hold new work there**"), [Line::Tell { to: None, item: Some("509".into()), text: "#509 moves hub/desk_*.rs for 2 hours | hold new work there".into() }]);
    assert!(lines("`TELL DESK: <Desk's name> | <words>`").is_empty() && lines("TELL DESK: loops").is_empty() && lines("TELL DESK: | words").is_empty());
}

#[test]
fn tell_desk_reaches_the_other_lead_as_the_band_and_is_noted_on_both_desks() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    let (_, lead_c) = elsewhere(&w);
    ends(&w, &lead_a, "ITEM: task | Fix the scraper");
    let id = item(&desk(&w, &a), "Fix the scraper")["id"].as_str().unwrap().to_string();
    assert_eq!(answer(&w, &lead_a, &format!("TELL DESK: feature b | #{id} is 20 min from landing. Land it first?")), "Told: Feature B.");
    let got = bands_to(&w, &lead_b);
    assert_eq!(got.len(), 1, "{got:?}");
    let band = got[0].rsplit("\n\n[From another Desk: ").next().unwrap();
    let words = format!("#{id} is 20 min from landing. Land it first?");
    assert!(band.starts_with("Feature A · lead ") && band.contains(&format!(" · about #{id}]\n{words}\n\n(To answer: TELL DESK: Feature A | <your words>.)")), "{band}");
    assert!(got[0].starts_with("<desk "), "after its Desk's summary, as Cue's words to a lead go: {}", got[0]);
    assert!(bands_to(&w, &lead_c).is_empty() && bands_to(&w, &lead_a).is_empty(), "no one else hears it");
    assert!(decisions(&w, &a).contains(&format!("Told Feature B's lead: {words}")), "{:?}", decisions(&w, &a));
    assert!(decisions(&w, &b).contains(&format!("Feature A's lead told this Desk: {words}")), "{:?}", decisions(&w, &b));
    let kept = w.hub().read(|st| st.desk.bands.clone());
    assert_eq!(kept.len(), 1);
    assert_eq!((kept[0].from.as_str(), kept[0].to.as_str(), kept[0].item.as_deref(), kept[0].text.as_str(), kept[0].ask, kept[0].answered), (a.as_str(), b.as_str(), Some(id.as_str()), words.as_str(), true, 0));
}

#[test]
fn tell_desks_reaches_every_other_lead_on_the_repository() {
    let w = World::new();
    let (_, lead_a) = start(&w, "Feature A");
    let (_, lead_b) = start(&w, "Feature B");
    let (_, lead_d) = start(&w, "Docs");
    let (_, lead_c) = elsewhere(&w);
    assert_eq!(answer(&w, &lead_a, "TELL DESKS: moving hub/desk_*.rs for about 2 hours. Hold new work there."), "Told: Docs, Feature B.");
    for lead in [&lead_b, &lead_d] {
        let got = bands_to(&w, lead);
        assert!(got.len() == 1 && got[0].contains("]\nmoving hub/desk_*.rs for about 2 hours. Hold new work there.\n\n(To answer: TELL DESK: Feature A |"), "{got:?}");
        assert!(!got[0].contains(" · about #"), "no item named");
    }
    assert!(bands_to(&w, &lead_c).is_empty(), "a Desk on another repository isn't told");
}

#[test]
fn tells_cue_cant_send_are_refused_with_what_to_do() {
    let w = World::new();
    let (_, lead_a) = start(&w, "Feature A");
    assert_eq!(answer(&w, &lead_a, "TELL DESKS: heads up"), "Refused: TELL DESKS: no other Desk with a lead is on this repository. You can carry on: nobody else works on this code.");
    let (_, lead_c) = elsewhere(&w);
    assert_eq!(answer(&w, &lead_a, "TELL DESK: Elsewhere | heads up"), "Refused: TELL DESK Elsewhere: no other Desk with a lead is on this repository. You can carry on: nobody else works on this code.");
    let (_, lead_b) = start(&w, "Feature B");
    assert_eq!(answer(&w, &lead_a, "TELL DESK: Elsewhere | heads up"), "Refused: TELL DESK Elsewhere: no such Desk on this repository. You can tell one of these: Feature B.");
    assert_eq!(answer(&w, &lead_a, "TELL DESK: Feature A | heads up"), "Refused: TELL DESK Feature A: it's your own Desk. You can tell another Desk on this repository: Feature B.");
    assert!(bands_to(&w, &lead_b).is_empty() && bands_to(&w, &lead_c).is_empty(), "nothing refused went");
    assert!(w.hub().read(|st| st.desk.bands.is_empty()));
}

#[test]
fn a_hold_tells_every_other_lead_on_the_repository() {
    let w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    let (_, lead_c) = elsewhere(&w);
    ends(&w, &lead_a, "ITEM: task | Move the Desk files\nHOLD: #new | area: hub/desk_*.rs, ui/desk_*.js | till: it lands | why: the move");
    let id = item(&desk(&w, &a), "Move the Desk files")["id"].as_str().unwrap().to_string();
    let held = sent_to(&w, &lead_a).into_iter().find(|t| t.starts_with("Held:")).unwrap();
    assert!(held.ends_with(" Told Feature B. No item on another Desk has changed a held file yet."), "{held}");
    let got = bands_to(&w, &lead_b);
    assert!(got.len() == 1 && got[0].contains(&format!(" · about #{id}]\nFeature A holds hub/desk_*.rs, ui/desk_*.js for #{id} till it lands (the move)\n\n")), "{got:?}");
    assert!(bands_to(&w, &lead_c).is_empty());
    assert!(decisions(&w, &b).iter().any(|d| d.starts_with("Feature A's lead told this Desk: Feature A holds")), "{:?}", decisions(&w, &b));
    let kept = w.hub().read(|st| st.desk.bands.clone());
    assert!(kept.len() == 1 && !kept[0].ask, "a heads-up, not a question: {kept:?}");
}

#[test]
fn a_tell_back_answers_the_open_bands_from_that_desk_and_bands_survive_a_relaunch() {
    let mut w = World::new();
    let (a, lead_a) = start(&w, "Feature A");
    let (b, lead_b) = start(&w, "Feature B");
    let (d, lead_d) = start(&w, "Docs");
    ends(&w, &lead_a, "TELL DESK: Feature B | Land yours first?");
    ends(&w, &lead_a, "TELL DESK: Docs | And yours?");
    let open = |w: &World, from: &str, to: &str| w.hub().read(|st| st.desk.bands.iter().filter(|x| x.from == from && x.to == to && x.ask && x.answered == 0).count());
    assert_eq!((open(&w, &a, &b), open(&w, &a, &d)), (1, 1));
    w.advance(60_000);
    assert_eq!(answer(&w, &lead_b, "TELL DESK: Feature A | Yes, land first."), "Told: Feature A.");
    assert_eq!((open(&w, &a, &b), open(&w, &a, &d), open(&w, &b, &a)), (0, 1, 1), "B's answer closes A's band to B only, and is a band of its own");
    assert!(bands_to(&w, &lead_a).iter().any(|t| t.contains("]\nYes, land first.")));
    let _ = lead_d;
    w.relaunch();
    assert_eq!((open(&w, &a, &b), open(&w, &a, &d), open(&w, &b, &a)), (0, 1, 1), "kept over a relaunch");
}
