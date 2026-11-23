// Cue UI board: a session's question card (ui/board/board.js has the rest of the Board).
// ---------- a session's question (#383, option C of the "Question card" mocks) ----------
// Cue picks the answers it suggests (its "(Recommended)" ones) for you, several questions are tabs (#461), each answer with
// what it does, and a line says what will happen; nothing goes until you press Send. Folded (a Board row, the Desk, the
// mini panel): "Send its suggestions", or Choose… to open the whole card.
/** The answers it suggests for question `q`: the labels it marked "(Recommended)". */
const suggestedOf = (q) => (q.options || []).filter((o) => choiceLabel(o.label)[1]).map((o) => o.label);
/** Whether it suggests an answer for every question. */
const hasSuggestions = (it) => questions(it).length > 0 && questions(it).every((q) => suggestedOf(q).length);
/** A question ask's draft, its suggestions picked for each question you haven't touched (what you change stays: a
 *  pick-any question you emptied keeps its empty set). */
function qDraft(it) {
  const d = draft(it.id);
  for (const q of questions(it)) {
    const s = suggestedOf(q);
    if (s.length && !(q.question in d.choices)) d.choices[q.question] = q.multiSelect ? new Set(s) : s[0];
  }
  return d;
}
/** What's picked for question `q`, as labels ("(Recommended)" kept, as the session wrote them). */
function qPicked(it, q) {
  const c = qDraft(it).choices[q.question];
  return q.multiSelect ? [...(c || [])] : c ? [c] : [];
}
function answersFor(it) {
  const out = {};
  for (const q of questions(it)) {
    const picked = qPicked(it, q);
    if (!picked.length) return null;
    out[q.question] = picked.join(", ");
  }
  return out;
}
const qStep = {};           // item id -> the question whose tab is open (where a typed answer and 1–9 go)
const isAnswered = (it, q) => qPicked(it, q).length > 0;
/** The question your keys and typed answer are for: the tab you opened (or last picked in), else the first not
 *  answered, else the first. */
function stepOf(it) {
  const qs = questions(it);
  const first = qs.findIndex((q) => !isAnswered(it, q));
  return Math.min(qStep[it.id] ?? Math.max(first, 0), Math.max(qs.length - 1, 0));
}
/** Pick choice `oi` of question `qi` (pick any: on or off). Nothing is sent: Send does that. */
function pick(it, qi, oi) {
  const q = questions(it)[qi];
  const label = q?.options?.[oi]?.label;
  if (!label) return;
  const d = qDraft(it);
  if (q.multiSelect) { const s = (d.choices[q.question] instanceof Set ? d.choices[q.question] : (d.choices[q.question] = new Set())); s.has(label) ? s.delete(label) : s.add(label); }
  else d.choices[q.question] = label;
  qStep[it.id] = qi;
  render();
}
/** Whether what's picked is just what it suggested. */
const qAsSuggested = (it) => hasSuggestions(it) && questions(it).every((q) => { const p = qPicked(it, q), s = suggestedOf(q); return p.length === (q.multiSelect ? s.length : 1) && p.every((l) => s.includes(l)); });
/** What's picked, in a line: each question's answers (", "), the questions apart ("; "). */
const qPickedLine = (it) => questions(it).map((q) => qPicked(it, q).map((l) => choiceLabel(l)[0]).join(", ") || "nothing yet").join("; ");
/** Send's label: "Send answer", "Send 2 answers", or how many are still to answer (then it's off). */
function qSendLabel(it) {
  const qs = questions(it), n = qs.length;
  if (answersFor(it)) return n > 1 ? `Send ${n} answers` : "Send answer";
  return n > 1 ? `${qs.filter((q) => isAnswered(it, q)).length} of ${n} answered` : "Pick an answer";
}
const qSendBtn = (it, cls = "btn primary") => `<button class="${cls}" data-act="submit-answers" data-id="${esc(it.id)}" ${answersFor(it) ? "" : "disabled"}>${esc(qSendLabel(it))}</button>`;
/** A folded question's line under it: "Ready with its suggestions: …" (or your answers, once you changed them);
 *  "" when not every question has an answer. With several questions, their names first. */
function qFoldLine(it) {
  const qs = questions(it);
  const names = qs.length > 1 ? `<span class="qc-tag">${qs.length} questions</span>${esc(qs.map((q, n) => q.header || `Question ${n + 1}`).join(" · "))} ` : "";
  const ready = answersFor(it) ? `${qAsSuggested(it) ? "Ready with its suggestions" : "Ready with your answers"}: ${esc(qPickedLine(it))}.` : "";
  return names || ready ? `<div class="qc-fold">${names}${names && ready ? "<br>" : ""}${ready}</div>` : "";
}
/** A folded question's buttons: Send its suggestions (or your answers), when every question has one, then `choose`
 *  (the surface's own way to open the whole card). `sendAttr`: Send's click, where a surface has its own. */
function qFoldBtns(it, choose, cls = "btn", sendAttr = `data-act="submit-answers" data-id="${esc(it.id)}"`) {
  const send = answersFor(it) ? `<button class="${cls} primary" ${sendAttr}${answering.has(it.id) ? " disabled" : ""}>${qAsSuggested(it) ? "Send its suggestions" : questions(it).length > 1 ? `Send ${questions(it).length} answers` : "Send answer"}</button>` : "";
  return send + choose;
}
/** The whole question card. Several questions: one tab per question (✓ once it has an answer), the open one's choices
 *  under them (#461); the open tab is where 1–9 and a typed answer go, and Send sends every answer. Each answer a row
 *  (its key, a radio or a box, its label, a Recommended tag, what it does). `attr`: the click that picks. */
function questionHtml(it, attr = "data-pick") {
  const qs = questions(it), lit = stepOf(it), many = qs.length > 1;
  const tabAttr = "data-qstep";
  const tabs = many ? `<div class="qc-tabs" role="tablist">${qs.map((q, qi) => {
    const done = isAnswered(it, q);
    return `<button role="tab" class="qc-tab${qi === lit ? " on" : ""}${done ? " done" : ""}" aria-selected="${qi === lit}" ${tabAttr}="${esc(it.id)}:${qi}" aria-label="${esc(`Question ${qi + 1}${done ? ", answered" : ""}`)}"><span class="qc-tn">${done ? "✓" : qi + 1}</span>${esc(q.header || `Question ${qi + 1}`)}</button>`;
  }).join("")}</div>` : "";
  const sec = (q, qi) => {
    const picked = qPicked(it, q);
    // A Desk lead's older asks carry "Desk" as their header: the card's head already names the lead (#450).
    const header = it.desk && q.header === "Desk" ? "" : q.header;
    const head = !many && header ? `<div class="qc-sn">${esc(header)}${q.multiSelect ? " · pick any" : ""}</div>` : "";
    const rows = (q.options || []).map((o, oi) => {
      const on = picked.includes(o.label), [label, rec] = choiceLabel(o.label);
      return `<button class="qc-opt${rec ? " rec" : ""}${on ? " on" : ""}" ${attr}="${qi}:${oi}" data-id="${esc(it.id)}" aria-pressed="${on}"><span class="qc-k">${oi < 9 ? oi + 1 : ""}</span><span class="qc-l"><span class="qc-mark ${q.multiSelect ? "chk" : "rad"}" aria-hidden="true"></span><span class="qc-lt">${esc(label)}</span>${rec ? `<span class="qc-rec">Recommended</span>` : ""}</span>${o.description ? `<span class="qc-d">${esc(o.description)}</span>` : ""}</button>`;
    }).join("");
    // An answer you typed yourself (not one of the choices) shows under them.
    const typed = picked.filter((c) => !(q.options || []).some((o) => o.label === c));
    const yours = typed.length ? `<div class="typed-answer">Your answer: “${esc(typed.join(", "))}”</div>` : "";
    return `<div class="qc-sec" role="${many ? "tabpanel" : "group"}">${head}<div class="qtext">${linkify(esc(q.question), sessBase(it.session_id))}${q.multiSelect && !head ? `<span class="dim"> · pick any</span>` : ""}</div><div class="qc-opts">${rows}</div>${yours}</div>`;
  };
  return `<div class="qc">${tabs}${qs.length ? sec(qs[lit], lit) : ""}</div>`;
}
