// Cue UI — an MCP server's form, asked by a hosted session (its "input" card): what the server says, one field
// per thing it asks for, and Decline or Send. The fields keep what you type as drafts, so a redraw never loses it.

/** The form's fields, from its JSON schema: [name, schema], in the server's order. */
const formFields = (it) => Object.entries(it.tool_input?.form?.properties || {});
const formKey = (it, name) => `form:${it.id}:${name}`;

/** One field: a choice of the server's values, yes or no, a number, or text. */
function formField(it, [name, p], required) {
  const key = formKey(it, name), val = draft(key).text;
  const attrs = `class="nf-in" id="${esc(key)}" data-text="${esc(key)}" aria-label="${esc(p.title || name)}"`;
  const choice = (opts) => `<select ${attrs}><option value=""></option>${opts.map(([v, l]) => `<option value="${esc(v)}" ${val === v ? "selected" : ""}>${esc(l)}</option>`).join("")}</select>`;
  const input = p.enum ? choice(p.enum.map((v) => [String(v), String(v)]))
    : p.type === "boolean" ? choice([["yes", "Yes"], ["no", "No"]])
    : `<input ${attrs} type="${p.type === "number" || p.type === "integer" ? "number" : "text"}" spellcheck="false" autocomplete="off" value="${esc(val)}"/>`;
  return `<div class="form-row"><label for="${esc(key)}">${esc(p.title || name)}${required ? "" : ` <span class="dim">optional</span>`}</label>${input}${p.description ? `<span class="dim">${esc(p.description)}</span>` : ""}</div>`;
}

function formBody(it) {
  const required = new Set(it.tool_input?.form?.required || []);
  return `<div class="qtext">${esc(it.tool_input?.message || "It asks you to fill this in.")}</div><div class="form-fields">${formFields(it).map((f) => formField(it, f, required.has(f[0]))).join("")}</div>`;
}

/** What you filled in, typed as the form's schema says; null while a required field is empty. */
function formValues(it) {
  const required = new Set(it.tool_input?.form?.required || []);
  const out = {};
  for (const [name, p] of formFields(it)) {
    const v = draft(formKey(it, name)).text.trim();
    if (!v) { if (required.has(name)) return null; continue; }
    out[name] = p.type === "boolean" ? v === "yes" : p.type === "number" || p.type === "integer" ? Number(v) : v;
  }
  return out;
}

const formButtons = (it) => `<div class="row-btns"><button class="btn deny flex" data-act="deny" data-id="${esc(it.id)}">Decline</button><button class="btn primary flex" data-act="form-send" data-id="${esc(it.id)}">Send</button></div>`;

/** Send: accepted with what you filled in (the hosted session hands it to the server), its drafts let go. */
function formSend(it) {
  const values = formValues(it);
  if (!values) return toast("Fill in the fields it needs first");
  formFields(it).forEach(([name]) => delete drafts[formKey(it, name)]);
  return respond(it, { behavior: "allow", answers: values });
}
