// Cue UI core: a message's words (summary, plain lines) and its markdown (md, links, code blocks, DECIDED lines).
const finishedText = (it) => it.message || it.context?.at(-1)?.text || "Finished. Your turn.";
/** The message as one plain line of prose, for previews (no markdown punctuation). */
const plain = (s) => String(s ?? "").replace(/```[\s\S]*?```/g, " ").replace(/^(\s*(?:[-*•]\s+)?)DECIDED:/gm, "$1Decided:").replace(/[`*#]/g, "").replace(/^\s*(>\s?)+/gm, "").replace(/^\s*[-•]\s+/gm, "").replace(/\s+/g, " ").trim();

function summary(it) {
  const i = it.tool_input || {};
  if (it.kind === "question") return questions(it)[0]?.question || "Question";
  if (it.kind === "input") return it.tool_input?.message || "A form to fill in";
  if (it.kind === "waiting") return firstLine(finishedText(it));
  if (inTerminal(it) && !it.tool_name) return it.message || "Asking in its terminal";
  switch (tool(it)) {
    case "bash": return i.command || "bash";
    case "edit": case "multiedit": case "write": case "notebookedit": return `${it.tool_name} ${shortPath(i.file_path || i.path || "")}`;
    case "webfetch": return `Fetch ${i.url || ""}`;
    case "apply_patch": return `Edit ${shortPath(i.file_path || "")}${(i.files || []).length > 1 ? ` and ${i.files.length - 1} more` : ""}`;
    case "exitplanmode": return typeof i.plan === "string" ? `Plan: ${plain(firstLine(i.plan))}` : "A plan to approve";
    default: return it.tool_name || "Request";
  }
}
function verb(it) {
  if (it.kind === "question") return "asks";
  if (it.kind === "input") return it.tool_name ? `has a form for you from ${it.tool_name}` : "has a form for you";
  if (inTerminal(it)) return "asks in its terminal";
  return { bash: "wants to run", edit: "wants to edit", multiedit: "wants to edit", write: "wants to write", webfetch: "wants to fetch", apply_patch: "wants to edit", exitplanmode: "has a plan for you" }[tool(it)] || `wants ${it.tool_name}`;
}
function describeSuggestion(s) {
  const where = { session: "this session", localSettings: "this project, just you", projectSettings: "this project", userSettings: "everywhere" }[s.destination] || s.destination || "";
  if (s.type === "addRules") return { title: `Always allow ${(s.rules || []).map((r) => (r.ruleContent ? `${r.toolName}(${r.ruleContent})` : r.toolName)).join(", ")}`, where };
  if (s.type === "addDirectories") return { title: `Allow access to ${(s.directories || []).map(shortPath).join(", ")}`, where };
  if (s.type === "setMode") return { title: `Switch to ${s.mode} mode`, where };
  return { title: s.type, where };
}

/** A Markdown table (`| a | b |` lines; an optional `|---|` row makes the line above it the header). */
function table(lines, inline) {
  const cells = (l) => l.trim().replace(/^\||\|$/g, "").split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, "|"));
  const SEP = /^\s*\|?(\s*:?-+:?\s*\|)*\s*:?-+:?\s*\|?\s*$/;
  const head = lines.length > 1 && SEP.test(lines[1]) ? cells(lines[0]) : null;
  const body = (head ? lines.slice(2) : lines).filter((l) => !SEP.test(l)).map(cells);
  const row = (cs, tag) => `<tr>${cs.map((c) => `<${tag}>${inline(c)}</${tag}>`).join("")}</tr>`;
  return `<div class="md-table"><table>${head ? `<thead>${row(head, "th")}</thead>` : ""}<tbody>${body.map((cs) => row(cs, "td")).join("")}</tbody></table></div>`;
}

/** What linkify links paths from: session `sid`'s folder, its machine and itself; null with no such session. */
const sessBase = (sid) => { const s = sid ? sessionOf(sid) : null; return s ? { cwd: s.cwd, machine: s.machine, sid } : null; };
/** Clickable links in already-escaped text: [label](url) and bare URLs, and paths (all opened by open_link on a
 *  click). `base` is the session the text came from: a path relative to its folder (src/main.rs, ./notes.md)
 *  links too, joined to that folder; on another machine no path links at all (it isn't on this Mac, and
 *  looking it up here can stall). Links are set aside as they're made, so a later pass can't reach into one. */
function linkify(h, base = null) {
  const links = [];
  const keep = (html) => `\u0000${links.push(html) - 1}\u0000`;
  const a = (u, label) => `<a href="#" data-href="${u}">${label}</a>`;
  // A path opens on a click like a link: a document in its app, code as text, anything else shown in Finder
  // (settings.rs how_to_open decides, from the real file); a markdown file in Cue's reader (its session's, so
  // it carries which session said it).
  const sid = base?.sid ? ` data-sid="${esc(base.sid)}"` : "";
  const pa = (path, shown) => `<a href="#" data-path="${path}"${sid}>${shown}</a>`;
  const remote = !!base?.machine;
  const trail = (raw) => raw.match(/[.,;:!?]+$/)?.[0] || "";
  let out = h.replace(/\[([^\]\n]+)\]\(((?:https?|file):\/\/[^\s)]+)\)/g, (_, label, u) => keep(a(u, label)))
    .replace(/(?:https?|file):\/\/[^\s<>"'\u0000]*[^\s<>"'.,;:!?)\]\u0000]/g, (u) => keep(a(u, u)));
  if (remote) return out.replace(/\u0000(\d+)\u0000/g, (_, n) => links[n]);
  // A picture in markdown (![what](/a/shot.png)): its words as a link to it; the picture itself shows under the message.
  out = out.replace(/!\[([^\]\n]*)\]\((\/[^\s)\u0000]+)\)/g, (_, alt, p) => keep(pa(p, alt || p)));
  out = out.replace(/(^|[^\w@/\\<.])(\/[\w.@~+-]+(?:\/[\w.@~+-]+)*\/?|~\/[^\s<>"'`\u0000]+)/g, (m, pre, raw) => {
    const t = trail(raw), p = raw.slice(0, raw.length - t.length);
    return pre + keep(pa(p, p)) + t;
  });
  // Relative to the session's folder: ./x or ../x, or a/b.ext (a slash and a file's extension, so "and/or" stays words).
  if (base?.cwd) {
    const dir = base.cwd.replace(/\/+$/, "");
    out = out.replace(/(^|[^\w@/\\<.:\u0000-])((?:\.{1,2}\/)[\w.@+-]+(?:\/[\w.@+-]+)*\/?|[\w@+-][\w.@+-]*(?:\/[\w.@+-]+)*\/[\w@+-][\w.@+-]*\.[A-Za-z][\w]{0,7})(?=$|[^\w/])/g, (m, pre, raw) => {
      const t = trail(raw), p = raw.slice(0, raw.length - t.length);
      if (/^[\w-]+(\.[\w-]+)+\//.test(p) && !/^\.\.?\//.test(p)) return m;   // example.com/page.html: a site, not a file here
      return pre + keep(pa(`${dir}/${p.replace(/^\.\//, "")}`, p)) + t;
    });
    // A markdown file by name alone (HANDOFF.md, the plan.md): the reader finds which one (ipc/docs.rs).
    out = out.replace(/(^|[^\w@/\\<.:\u0000-])([\w@+-][\w.@+-]*\.(?:md|markdown))(?=$|[^\w/])/g, (m, pre, p) => pre + keep(pa(`${dir}/${p}`, p)));
  }
  return out.replace(/\u0000(\d+)\u0000/g, (_, n) => links[n]);
}
/** Markdown-lite for agents' messages: paragraphs, lists, tables, headings, quotes, code. Escapes first, so it's safe. */
/** Copy on a code block or a command: what's in the block beside it, as plain text (a "$ " before a command isn't). */
/** A permission's long command or request (#426): cut to a few lines with "more", which opens the rest in place, and
 *  "less", which folds it back; a short one shows in full. `key` is the request's id: open stays open through redraws,
 *  and on every card that shows it. */
const reqOpen = new Set();
function reqCut(key, inner, text, lines = 3, chars = 180) {
  const t = String(text ?? "");
  if (t.split("\n").length <= lines && t.length <= chars) return inner;
  const open = reqOpen.has(key);
  return `<div class="req-cut${open ? " open" : ""}" style="--cut-lines:${lines}">${inner}<button class="req-more" data-req-more="${esc(key)}" aria-expanded="${open}">${open ? "less" : "more"}</button></div>`;
}
/** After a draw: a cut whose words fit in their lines after all (reqCut guesses by length; a wide window fits more)
 *  hides its More, so More always has more under it (#452). */
function reqFit() {
  for (const t of document.querySelectorAll(".req-cut:not(.open) > :is(.req-t, .code-wrap > pre)")) t.closest(".req-cut").classList.toggle("fits", t.scrollHeight <= t.clientHeight + 1);
}
// Before any row's own click (capture): more / less opens or folds just the words, not the row it sits on.
document.addEventListener("click", (e) => {
  const b = e.target.closest?.("[data-req-more]");
  if (!b) return;
  // Immediate: the Desk's own click listener sits on the document too, and would open the row.
  e.preventDefault(); e.stopImmediatePropagation();
  const key = b.dataset.reqMore, open = !reqOpen.has(key);
  if (open) reqOpen.add(key); else reqOpen.delete(key);
  for (const btn of document.querySelectorAll(`[data-req-more="${CSS.escape(key)}"]`)) {
    btn.closest(".req-cut")?.classList.toggle("open", open);
    btn.textContent = open ? "less" : "more";
    btn.setAttribute("aria-expanded", String(open));
  }
}, true);
const COPY_BTN = `<button class="code-copy" data-copy-code aria-label="Copy">Copy</button>`;
/** A shell command someone asks to run, as every card draws it (#670): the chat's script block (a slab, "$ ", Copy),
 *  cut to its first lines with a fade and more / less when long. `key`: what keeps it open (reqCut). The chat's
 *  permission card, the Desk's Away asks and its held steps all use this one, so they look the same. */
const cmdCut = (key, cmd) => reqCut(key, `<div class="code-wrap"><pre class="slab"><span class="ps">$ </span>${esc(cmd)}</pre>${COPY_BTN}</div>`, cmd);
/** The text of the code block a button sits on, without a "$ " prompt Cue drew before a command. */
function codeText(btn) {
  const pre = btn.closest(".code-wrap")?.querySelector("pre")?.cloneNode(true);
  if (!pre) return "";
  pre.querySelectorAll(".ps").forEach((el) => el.remove());
  return pre.textContent;
}
function copyCode(btn) {
  const text = codeText(btn);
  if (text.trim()) invoke("copy_text", { text }).then(() => toast("Copied"), () => toast("Couldn't copy"));
}
/** A code block the agent marked as shell commands: Run beside Copy, in a live session's chat (md's `runIn`). */
const SHELL_LANG = /^(bash|sh|zsh|shell|console|terminal)$/;
const runBtn = (sid) => `<button class="code-run" data-run-code="${esc(sid)}" aria-label="Run in its folder">Run</button>`;
/** Run: the block's lines in the session's folder, as "! " would (chat/send.js runLine); its card shows in the chat.
 *  A "$ " the agent put before a command is dropped, as in a terminal. */
function runCode(btn) {
  const line = codeText(btn).split("\n").map((l) => l.replace(/^\$ /, "")).join("\n").trim();
  if (line) runLine(btn.dataset.runCode, line);
}
/** A fenced code block (`chunk`, its language on its first line) as drawn: long code folds to ~12 lines (scrolls
 *  inside), so a dump doesn't push the conversation away; shell code in a session (`runIn`) gets Run. */
function codeBlock(chunk, runIn) {
  const lang = /^([a-z]*)\n/.exec(chunk)?.[1] || "";
  const code = chunk.replace(/^[a-z]*\n/, "").replace(/\n$/, "");
  const n = code.split("\n").length;
  const run = runIn && SHELL_LANG.test(lang) && code.trim() ? runBtn(runIn) : "";
  return `<div class="code-wrap">${n > 12 ? `<pre class="long">${esc(code)}</pre><div class="code-more">${n} lines · scroll inside</div>` : `<pre>${esc(code)}</pre>`}${run}${COPY_BTN}</div>`;
}
/** What a `lines` hook gives for a line of its page's own that's shown elsewhere, not in the text (the lead's ASK: its
 *  card is in the chat already). */
const MD_DROP = "<!--drop-->";
/** `lines`: a hook for lines of a page's own (the Desk's lead lines, desk/lead.js deskLineHtml): (line, inline) → its
 *  html, MD_DROP to leave it out, or "" for an ordinary line. Lines in code blocks never reach it; a run of them is one block.
 *  `run`: a hook for each such block drawn (and, with `lines`, each block of decisions): its html → what goes in its place. */
/** `refs`: a hook for each line's words once drawn (html, links already made) → them with a page's own links added
 *  (the lead chat's items and sessions, desk/lead.js deskRefs). Never given code blocks. */
/** `cue`: Cue's own lines (cueLineHook) out of the words, as every chat has them; off for a file (`doc`) or a page with
 *  its own `lines`. */
function md(text, { runIn = null, pathsOf = null, doc = false, lines = null, run = null, refs = null, cue = true } = {}) {
  const out = [];
  const cueHook = cue && !doc && !lines ? cueLineHook(text) : null;
  // Its folder for relative paths, its machine (another: no paths): the session it runs in, or (pathsOf) only
  // whose paths these are, with no Run on its commands (the mini window).
  const base = sessBase(runIn || pathsOf);
  const inline = (s) => { const h = linkify(esc(s).replace(/`([^`]+)`/g, "<code>$1</code>").replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>"), base); return refs ? refs(h) : h; };
  // A Desk reply's "#new" as the number of the item it filed (desk/lead.js deskFillNew, #934).
  if (!doc && typeof deskFillNew === "function") text = deskFillNew(text);
  const blocks = String(text ?? "").replace(/\r/g, "").split(/```/);
  blocks.forEach((chunk, i) => {
    if (i % 2 === 1) {
      out.push(codeBlock(chunk, runIn));
      return;
    }
    const LIST = /^\s*([-*•]|\d+[.)])\s+/;
    const TABLE = /^\s*\|.*\|\s*$/;
    const QUOTE = /^\s*>\s?/;
    // "> " lines are a quote: a line down its side, as in Slack, with Markdown inside.
    for (const para of chunk.split(/\n\s*\n/)) {
      // A paragraph can mix a heading line, text lines and list lines: split it into runs.
      const runs = [];
      for (const l of para.split("\n").filter((x) => x.trim())) {
        const own = lines ? lines(l, inline) : cueHook ? cueHook(l) : "";
        if (own === MD_DROP) continue;
        if (own) { if (runs.at(-1)?.kind === "own") runs.at(-1).lines.push(own); else runs.push({ kind: "own", lines: [own] }); continue; }
        const kind = DECIDED_LINE.test(l) ? "decided" : QUOTE.test(l) ? "quote" : TABLE.test(l) ? "table" : LIST.test(l) ? "list" : /^#{1,4}\s/.test(l) || /^\*\*[^*]+\*\*:?$/.test(l.trim()) ? "head" : "text";
        const last = runs.at(-1);
        // In Read (doc), a wrapped line under a list item goes on with that item, as Markdown has it.
        if (doc && last?.kind === "list" && kind === "text") { last.lines[last.lines.length - 1] += ` ${l.trim()}`; continue; }
        if (last && last.kind === kind && kind !== "head") last.lines.push(l);
        else runs.push({ kind, lines: [l] });
      }
      for (const r of runs) {
        if (r.kind === "own") {
          const h = `<div class="own-lines">${r.lines.join("")}</div>`;
          // `run` gets its lines too, one by one (desk/runs.js takes the Desk's out without parsing the block).
          out.push(run ? run(h, r.lines) : h);
        } else if (r.kind === "decided") {
          // With `lines` (a page's own), its decisions are its own lines too (`run`: a page's way to fold them).
          const h = decidedHtml(r.lines, inline, !!lines);
          out.push(run && lines ? run(h) : h);
        } else if (r.kind === "quote") {
          out.push(`<blockquote>${md(r.lines.map((l) => l.replace(QUOTE, "")).join("\n"), { refs })}</blockquote>`);
        } else if (r.kind === "table") {
          out.push(table(r.lines, inline));
        } else if (r.kind === "list") {
          out.push(listHtml(r.lines, LIST, inline));
        } else if (r.kind === "head") {
          // A file in Read (doc): its level kept (# 1, ## 2…; a bold line 0), for its size and its Contents.
          const lv = doc ? ` class="dh${r.lines[0].match(/^#+/)?.[0].length || 0}"` : "";
          out.push(`<h4${lv}>${inline(r.lines[0].replace(/^#+\s/, "").replace(/\*\*/g, ""))}</h4>`);
        } else {
          // A file's lines wrap where its writer wrapped them: in Read (doc) a paragraph flows, as Markdown has it.
          out.push(`<p>${r.lines.map(inline).join(doc ? " " : "<br>")}</p>`);
        }
      }
    }
  });
  return out.join("");
}

/** A line a session writes for Cue to act on, as desk/parse.rs reads it (a list mark or bold around its word): DONE,
 *  CHECK, IF PICKED, DESIGNS, TO LEAD… Cue reads these, so no chat shows them as written (#833). Its { word, body },
 *  null for any other line, or one Cue wouldn't act on (an empty or "<…>" body, no item number where one goes). */
const CUE_LINE = /^[-*+ ]*(?:\d+[.)]\s+)?\**(DONE|NOT DONE|CHECK|IF PICKED|PICKED|DESIGNS|SEE|ANSWER|PASS|ITEM|TO LEAD|USER VERIFY|DECIDED \(minor\))\**\s*:(.*)$/;
const CUE_NUMBERED = new Set(["DONE", "NOT DONE", "CHECK", "IF PICKED", "PICKED", "DESIGNS", "SEE", "ANSWER", "PASS"]);
function cueLineOf(line) {
  const m = CUE_LINE.exec(String(line || "").trim());
  if (!m) return null;
  const body = m[2].trim().replace(/^\*\*/, "").trim().replace(/\*+$/, "").trim();
  if (!body || body.startsWith("<")) return null;
  if (CUE_NUMBERED.has(m[1]) && !body.startsWith("#")) return null;
  if (m[1] === "ITEM" && !/^(bug|feature|task)\s*\|/i.test(body)) return null;
  return { word: m[1], body };
}
/** A DESIGNS line's link ("#833 https://…", as parse.rs designs), "" for none. */
const cueDesignsUrl = (body) => /^#\S+\s.*?(https?:\/\/[^\s>)\]"`*]+)/.exec(body)?.[1]?.replace(/[.,;]+$/, "") || "";
/** The mocks' link as a button (a DESIGNS line, an IF PICKED line's). */
const cueMocksBtn = (url) => `<a class="mg-btn cue-mocks" href="#" data-href="${esc(url)}">Open the mocks ↗</a>`;
/** The one filter every chat draws a reply through (md's own `lines` when a page gives none; #833): what `text`'s Cue
 *  lines become, line by line. DESIGNS: an Open the mocks button. IF PICKED: its first, one quiet line, how many options
 *  and (with no DESIGNS line in the reply) the item's mocks; the rest left out. Every other one (DONE, CHECK, TO LEAD…)
 *  left out: Cue shows it where it acts (CHECK on its item's check card). null when `text` has none. */
function cueLineHook(text) {
  const t = String(text ?? "");
  if (!/\b(?:DONE|CHECK|IF PICKED|PICKED|DESIGNS|SEE|ANSWER|PASS|ITEM|TO LEAD|USER VERIFY|DECIDED \(minor\))\**\s*:/.test(t)) return null;
  const all = t.split(/```/).filter((_, i) => i % 2 === 0).join("\n").split("\n").map(cueLineOf).filter(Boolean);
  if (!all.length) return null;
  const picks = all.filter((c) => c.word === "IF PICKED");
  const opts = new Set(picks.map((c) => (c.body.split("|")[1] || "").trim().toLowerCase()));
  const designs = all.some((c) => c.word === "DESIGNS" && cueDesignsUrl(c.body));
  let said = false;
  return (l) => {
    const c = cueLineOf(l);
    if (!c) return "";
    if (c.word === "DESIGNS") { const url = cueDesignsUrl(c.body); return url ? `<div class="cue-ln">${cueMocksBtn(url)}</div>` : MD_DROP; }
    if (c.word !== "IF PICKED" || said) return MD_DROP;
    said = true;
    const url = designs ? "" : cueItemMocks(c.body);
    const n = opts.size;
    return `<div class="cue-ln cue-pick"><span>${n} option${n === 1 ? "" : "s"} to pick from</span>${url ? ` · <a class="cue-mocks-a" href="#" data-href="${esc(url)}">Open the mocks ↗</a>` : ""}</div>`;
  };
}
/** The mocks of the item an IF PICKED line names ("#833 | B | …"), as its page links them (desk/page.js deskCanvas);
 *  "" outside the Desk or for none. */
function cueItemMocks(body) {
  const id = (/^#([a-z0-9]+)/i.exec(body)?.[1] || "").toLowerCase();
  if (!id || typeof deskCanvas !== "function" || typeof state === "undefined") return "";
  const it = (state?.desk?.projects || []).flatMap((d) => d.items || []).find((i) => (typeof deskIdIs === "function" ? deskIdIs(i, id) : i.id === id));
  return it ? deskCanvas(it) : "";
}

/** A judgment call an agent made on its own, as it writes it: `DECIDED: what — why` (a list mark before it is fine). */
const DECIDED_LINE = /^\s*(?:[-*•]\s+)?DECIDED:\s*/;
/** A run of DECIDED lines as one small block: "Decided on its own" and how many, then each call, its reason under it
 *  in grey (what follows its first " — ", " – " or " -- "). */
function decidedHtml(lines, inline, own = false) {
  const item = (l) => {
    const t = l.replace(DECIDED_LINE, ""), m = /\s+(?:—|–|--)\s+/.exec(t);
    const what = m ? t.slice(0, m.index) : t, why = m ? t.slice(m.index + m[0].length) : "";
    return `<li${own ? " data-dk-li" : ""}><span class="dec-what">${inline(what.replace(/[.;:]\s*$/, ""))}</span>${why ? `<span class="dec-why">${inline(why)}</span>` : ""}</li>`;
  };
  return `<div class="decided"${own ? ' data-dk="decided"' : ""}><div class="dec-head">Decided on its own${lines.length > 1 ? ` · ${lines.length}` : ""}</div><ul>${lines.map(item).join("")}</ul></div>`;
}

/** List lines as a list: numbered from its first item's number when that's numbered. A line indented further
 *  than the list's first is part of a list inside the item above it (a "1." item's "   - " points stay its own,
 *  not items 2, 3, 4 of the numbered list). */
function listHtml(lines, LIST, inline) {
  const indent = (l) => l.match(/^\s*/)[0].length;
  const base = indent(lines[0]);
  const items = [];
  for (const l of lines) {
    if (indent(l) > base && items.length) items.at(-1).sub.push(l);
    else items.push({ text: l.replace(LIST, ""), sub: [] });
  }
  const ordered = /^\s*\d/.test(lines[0]);
  const tag = ordered ? `ol start="${parseInt(lines[0], 10) || 1}"` : "ul";
  return `<${tag}>${items.map((it) => `<li>${inline(it.text)}${it.sub.length ? listHtml(it.sub, LIST, inline) : ""}</li>`).join("")}</${ordered ? "ol" : "ul"}>`;
}
