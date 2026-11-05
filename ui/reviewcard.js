// The Review card at the top of a lead's chat: one card per packet of its team that waits on you. The state of each
// comes from Cue (state.reviews, one entry per packet: state, what to show); this only draws it.

/** The heading of each state and the buttons that go with it, in order. */
const REVIEW_CARD = {
  waiting: { head: "Waiting for you", acts: ["review", "diff", "aside"] },
  auto: { head: "Waiting", acts: ["diff"] },
  queued: { head: "Reviewing", acts: ["diff"] },
  reviewing: { head: "Reviewing", acts: ["diff"] },
  reviewed: { head: "Reviewed · waiting for you", acts: ["commit", "fixes", "diff"] },
};

/** What a card says about its packet in each state. */
function reviewCardBody(r) {
  const name = `<b>${esc(r.name || "its executor")}</b>`, packet = r.packet ? ` packet ${r.packet}` : "";
  const sub = (text) => text ? `<div class="rv-sub">${esc(text)}</div>` : "";
  switch (r.state) {
    case "reviewing": return `${name}${packet}: running the tests and reading the diff against the packet.`;
    case "queued": return `${name}${packet}: review queued, it starts when the lead is free.`;
    case "reviewed": return `${name}${packet}${r.findings.length ? `<div class="rv-find">${r.findings.map((l) => `<div>${esc(l)}</div>`).join("")}</div>` : ": reviewed."}`;
    case "auto": return `${name} finished${packet}${r.outcome ? `: ${esc(r.outcome)}` : ""}.${sub("The lead is reviewing this on its own.")}`;
    default: return `${name} finished${packet}${r.goal ? `: ${esc(r.goal)}` : ""}.${sub(r.stat)}${sub(r.missed ? "Its lead hasn't picked this up." : "")}`;
  }
}

/** One button: Review, Diff and Set aside are Cue's (data-crew); Commit and Send fixes type their words into the lead. */
function reviewCardButton(act, exec, lead) {
  const say = (text, label, primary) => `<button class="btn ${primary ? "primary" : ""}" data-rv-say="${esc(lead)}" data-text="${esc(text)}">${label}</button>`;
  switch (act) {
    case "review": return crewBtn(exec, "review", "Review", true);
    case "diff": return crewBtn(exec, "diff", "Diff");
    case "aside": return crewBtn(exec, "aside", "Set aside");
    case "commit": return say("commit it", "Commit", true);
    default: return say("send the fixes", "Send fixes");
  }
}

/** The lead's cards, "" when nothing waits on you. */
function reviewCard(lead) {
  return Object.entries(state.reviews || {}).filter(([, r]) => r.lead === lead).map(([exec, r]) => {
    const card = REVIEW_CARD[r.state];
    return `<div class="rv-card"><div class="tc-h">${card.head.toUpperCase()}</div><div class="rv-body">${reviewCardBody(r)}</div><div class="tc-acts">${card.acts.map((a) => reviewCardButton(a, exec, lead)).join("")}</div></div>`;
  }).join("");
}
