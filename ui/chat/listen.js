// Cue UI chat — Listen: a reply read out loud, in the Mac's voice or a natural one.
// ---------- Listen: a reply read out loud ----------
// Listen under any reply (and in the reader) reads it out a sentence at a time in the Mac's voice, at 1× to 1.6× in
// small steps (remembered). The sentence it's on goes lime with the word it's saying underlined, what it read goes gray
// (CSS highlights over the reply as shown, so the chat's own HTML is untouched), and it scrolls along. A click on a
// sentence starts there. Code blocks are said as "Code block." once, a table a row at a time. In the chat one line
// above the box says it's listening (Pause, Stop); in the reader the foot is its player, and it goes on to the next
// reply when "Then the next reply" is on. Opening another session keeps it reading (from the reply as last shown);
// that session's line says whose reply it is, a click goes back, and there the marks are where it got to.
let listening = null;    // { sid, key, i (sentence), n, playing, reader (started there), html (the reply as last shown), all (the reader's replies), follow (the chat scrolls along), ahead (scrolled away, the reading above the mark) }
let listenRun = 0;       // bumped on every start, pause and stop: an old sentence's end is ignored
const LISTEN_SPEEDS = [1, 1.1, 1.25, 1.4, 1.6];
let listenSpeed = 0, listenThen = true, listenPlain = true;
try { listenSpeed = Math.max(0, LISTEN_SPEEDS.indexOf(Number(localStorage.getItem("cue.listenRate")))); listenThen = localStorage.getItem("cue.listenThen") !== "0"; listenPlain = localStorage.getItem("cue.listenPlain") !== "0"; } catch {}
/** "Skip code & tables" (on unless turned off): what makes no sense heard is left out. Tables and code blocks
 *  whole; in a sentence, a path, a link, code that isn't a plain word (a slash, a dot, an underscore, a brace…) and
 *  a "(§4.1)" reference, so the rest of it is read; a sentence with nothing left is passed over. */
const LISTEN_TECH = /^(?:-|.*\s-)|^(?:[\w-]*[/._{}()<>=:\\$#@|[\]][\S]*|\S{25,})$/;
function listenPlainOf(el, text, nodes) {
  // The characters to say: those not in technical code or a link that is a path or a web address. A path is said
  // by its last part, a word (src-tauri/src/hosted/ → "hosted", docs/plan.md → "plan"), so the sentence still reads.
  const keep = new Uint8Array(text.length).fill(1), subs = new Map(), named = new Set();
  for (const { n, at } of nodes) {
    const code = n.parentElement.closest("code"), link = n.parentElement.closest("a[data-path], a[data-href]");
    const t = (code || link)?.textContent.trim() || "";
    if (!((code && LISTEN_TECH.test(t)) || (link && (link.dataset.path || /^(?:https?|file):/.test(t))))) continue;
    keep.fill(0, at, at + n.data.length);
    const holder = link || code;
    const word = /^(?:https?|file):/.test(t) ? "" : t.includes("/") ? t.replace(/\/+$/, "").split("/").pop().replace(/\.[A-Za-z]\w{0,7}$/, "").replace(/[-_]+/g, " ") : "";
    if (word && /^[A-Za-z][\w ]*$/.test(word) && !named.has(holder)) { named.add(holder); subs.set(at, word); }
  }
  return { keep, subs };
}
function listenSay1(text, plain, a, b) {
  if (!plain) return text.slice(a, b).replace(/\s/g, " ");
  let s = "";
  for (let i = a; i < b; i++) s += plain.keep[i] ? text[i] : plain.subs.has(i) ? ` ${plain.subs.get(i)} ` : " ";
  return s.replace(/https?:\/\/\S+/g, " ").replace(/\(\s*§[\d.,–\s-]+\)|§[\d.]+/g, " ").replace(/[*_]{1,2}(?=\w)|(?<=\w)[*_]{1,2}/g, "")
    .replace(/[,;]\s*\)/g, ")").replace(/\(\s*[,;]?\s*\)/g, " ")
    .replace(/\s+([,.;:)])/g, "$1").replace(/\(\s+/g, "(").replace(/\s+/g, " ").replace(/^[\s,;:]+/, "").trim();
}
/** The "Skip code & tables" checkbox: one setting, shown in the reader, in Read and above the chat's box. */
const listenPlainBox = () => `<label class="ls-then rdv-plain"><input type="checkbox" data-listen-plain="1" ${listenPlain ? "checked" : ""}/>Skip code &amp; tables</label>`;
function listenPlainFlip() {
  listenPlain = !listenPlain;
  try { localStorage.setItem("cue.listenPlain", listenPlain ? "1" : "0"); } catch {}
  renderMain();
}
const LISTEN_BLOCKS = "p, li, h1, h2, h3, h4, h5, h6, pre, tr, blockquote";
/** The reply `key` as shown: in the reader when it's open on it, else its bubble in the chat. */
function listenRoot(key) {
  return [...document.querySelectorAll(".rd[data-rdkey] .msg")].find((m) => m.closest(".rd").dataset.rdkey === key)
    || [...document.querySelectorAll("[data-listen]")].find((b) => b.dataset.listen === key)?.closest(".cv-agent, .cu-strip")?.querySelector(".msg") || null;
}
/** What it reads from: the reply on screen, else (its session not open) a copy of it as it was last shown. */
function listenSource(l) {
  const root = listenRoot(l.key);
  if (root) { l.html = root.innerHTML; return root; }
  if (l.html == null) return null;
  const el = document.createElement("div");
  el.innerHTML = l.html;
  return el;
}
/** A reply's sentences as shown, in reading order: each its text (what's said), and a Range over it when it's
 *  marked (not a table row or a code block: those keep their element, for following them). */
function listenSents(root) {
  const out = [];
  const els = [...root.querySelectorAll(LISTEN_BLOCKS)].filter((el) => !(el.matches("blockquote") && el.querySelector(LISTEN_BLOCKS)));
  for (const el of els.length ? els : [root]) {
    if (el.matches("pre")) { if (!listenPlain && !el.parentElement.closest("pre")) out.push({ say: "Code block.", el }); continue; }
    if (el.matches("tr") && listenPlain) continue;
    if (el.matches("tr")) { const say = [...el.cells].map((c) => c.textContent.trim()).filter(Boolean).join(", "); if (/\w/.test(say)) out.push({ say, el }); continue; }
    // This block's own text (not a nested list's or a code block's), as one string over its text nodes.
    const nodes = [], walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, { acceptNode: (n) => (n.parentElement.closest(LISTEN_BLOCKS) === el ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT) });
    let text = "";
    for (let n; (n = walk.nextNode());) { nodes.push({ n, at: text.length }); text += n.data; }
    const keep = listenPlain ? listenPlainOf(el, text, nodes) : null;
    const pos = (i) => { let k = nodes.length - 1; while (k > 0 && nodes[k].at > i) k--; return [nodes[k].n, Math.min(i - nodes[k].at, nodes[k].n.data.length)]; };
    // A heading is said whole ("1. What and why", not "1." and then the rest).
    const whole = /^H[1-6]$/.test(el.tagName) ? [Object.assign([text.trimEnd()], { index: 0 })] : null;
    for (const m of whole || text.matchAll(/(?:[^.!?\n]|[.!?]+(?=[^\s.!?"')\]]))+(?:[.!?]+["')\]]*(?=\s|$)|\n|$)/g)) {
      const a = m.index + (m[0].length - m[0].trimStart().length), b = m.index + m[0].trimEnd().length;
      if (b <= a || !/\w/.test(m[0])) continue;
      const range = document.createRange();
      range.setStart(...pos(a)); range.setEnd(...pos(b));
      const say = listenSay1(text, keep, a, b);
      if (!/\w/.test(say)) continue;
      out.push({ say, range, pos, a });
    }
  }
  return out;
}
/** The marks: read in gray, the sentence it's on in lime, the word at `w` ([char, length] in it) underlined. */
function listenMark(sents, i, w) {
  if (!CSS.highlights) return;
  const done = sents.slice(0, i).filter((x) => x.range).map((x) => x.range), on = sents[i];
  CSS.highlights.set("cue-said", new Highlight(...done));
  CSS.highlights.set("cue-say", new Highlight(...(on?.range ? [on.range] : [])));
  CSS.highlights.delete("cue-word");
  const r = listenWord(on, w);
  if (r) CSS.highlights.set("cue-word", new Highlight(r));
}
/** A Range over the word at `w` ([char, length]) in sentence `x`, when it's marked. */
function listenWord(x, w) {
  if (!x?.range || !w) return null;
  const r = document.createRange();
  r.setStart(...x.pos(x.a + w[0])); r.setEnd(...x.pos(x.a + w[0] + w[1]));
  return r;
}
function listenUnmark() { if (CSS.highlights) ["cue-said", "cue-say", "cue-word"].forEach((h) => CSS.highlights.delete(h)); }
/** The chat scrolls along with what's read: the line being heard (the word, else the sentence's first line, a code
 *  block or table row as a whole) is held a third down, the chat moving up a line as the reading goes down one.
 *  Above that mark it's left where it is (the start of a reply near the top), unless it's out of view. A step is
 *  smooth, a jump (more than a screen) instant. Scroll the chat yourself and it stops following (`l.follow`) until
 *  the reading comes down to the mark from above, or you skip, or click a sentence. */
function listenFollow(l) {
  // The page it scrolls: the chat, or Read's (read.js) for a file read there.
  const v = l && listenView(l), x = v?.sents[l.i], chat = v?.root.closest("[data-chat], .rdv-scroll");
  if (!x || !chat) return;
  const r = (listenWord(x, l.w) || x.range)?.getClientRects()[0] || x.el?.getBoundingClientRect();
  if (!r) return;
  const box = chat.getBoundingClientRect(), mark = box.top + box.height / 3;
  const over = r.top < box.top + 24 || r.bottom > box.bottom - 40, below = r.top > mark + 4;
  // Scrolled away: on again once the reading, above the mark (you went on ahead), comes down to it. Below the
  // mark (you went back) it's left there.
  if (l.follow === false) {
    if (!over && r.top < mark) l.ahead = true;
    if (over || r.top < mark || !l.ahead) return;
    l.follow = true;
  }
  if (!over && !below) return;
  // A glide of Cue's own (core.js glideTo), not the browser's smooth scroll: this runs in every redraw, and in
  // WebKit a smooth scroll started then ran from the chat's top.
  const by = r.top - mark;
  if (Math.abs(by) < box.height) return glideTo(() => listenRoot(l.key)?.closest("[data-chat], .rdv-scroll"), chat.scrollTop + by);
  glide = null;
  chat.scrollTop += by;
}
/** You scrolled the chat being read yourself (app/bind.js: the wheel or trackpad, or its scroll bar): it stops
 *  following, so it doesn't pull you back while you look elsewhere. */
function listenScrolled(chat) {
  const root = listening && listenRoot(listening.key);
  if (root && root.closest("[data-chat], .rdv-scroll") === chat) { listening.follow = false; listening.ahead = false; }
}
/** The reply being read as shown, with its sentences, worked out again only when asked (`fresh`: after a redraw)
 *  or when the reply on screen is another element: the word marks move many times a second. */
function listenView(l, fresh = false) {
  const root = listenRoot(l.key);
  if (!root) return (l.view = null);
  if (fresh || l.view?.root !== root) l.view = { root, sents: listenSents(root) };
  return l.view;
}
/** The marks where it is: the sentence `l.i` and the word in it (`l.w`, [at, length] in its words), if shown. */
function listenMarkNow(l, fresh = false) {
  const v = listenView(l, fresh);
  if (v) { listenMark(v.sents, l.i, l.w); listenFollow(l); }
}
/** Say the sentence it's on, then the next, to the end (and then the reader's next reply, when that's on). */
function listenSay() {
  const l = listening, root = l && listenSource(l);
  if (!root) return listenStop();
  const sents = listenSents(root), shown = root.isConnected;
  l.n = sents.length;
  if (l.i >= sents.length) return listenNext();
  const x = sents[l.i], run = ++listenRun;
  l.w = null;
  if (shown) { l.view = { root, sents }; listenMark(sents, l.i, null); listenFollow(l); } else listenUnmark();
  if (kokoroOn() && l.mac !== l.i) return kokoroSay(x, sents[l.i + 1], run);
  const u = new SpeechSynthesisUtterance(x.say);
  u.rate = LISTEN_SPEEDS[listenSpeed];
  u.onboundary = (e) => { if (run === listenRun && e.name === "word") { l.w = [e.charIndex, e.charLength || (x.say.slice(e.charIndex).match(/^\S+/)?.[0].length ?? 0)]; listenMarkNow(l); } };
  u.onend = () => { if (run !== listenRun) return; l.i++; listenSay(); };
  u.onerror = () => { if (run === listenRun) listenStop(); };
  speechSynthesis.cancel();
  speechSynthesis.speak(u);
  listenTell();
}
// ---------- Natural voices (Kokoro) ----------
// The voice Cue reads out in is a setting (voice.pick, Settings → Voice): "" the Mac's own (System Settings →
// Spoken Content), else a Kokoro voice. Each Kokoro voice is downloaded on its own (0.5 MB), the first with the
// voice engine they all use (about 125 MB), through cue-speak (Kokoro on this Mac, terminal/speak.rs). Listen
// then reads in it a sentence at a time, the next one made while this one plays; its marks follow where the
// sound is ({"at"}). If it fails or quits mid-sentence, Listen goes on in the Mac's voice from that sentence and
// says so once.
const voicePick = () => state.settings?.voice?.pick || "";
/** What cue-speak said of itself: the engine is here (`ready`), the voices here (`have`) and the space it all
 *  takes (`mb`), a macOS with the Apple bug (`risky`), none on this Mac (`none`: Intel); the voice being got and
 *  how far (`getting`, `progress`), how many sentences in a row it failed (`fails`) and that it gave up this run
 *  (`broke`). */
let kokoro = { known: false, ready: false, have: [], mb: 0, risky: false, none: false, getting: null, progress: 0, fails: 0, broke: false, bye: false };
const kokoroOn = () => !!voicePick() && kokoro.have.includes(voicePick()) && !kokoro.broke;
let kokoroAt = null;   // the sentence it's saying: { run, x, ms }
function kokoroTell(line) { invoke("speak_send", { line }).catch((e) => kokoroLine({ error: String(e), id: kokoroAt?.run, get: kokoro.getting })); }
function kokoroStatus(again = false) { if (again || !kokoro.known) { kokoro.known = true; kokoroTell({ status: true }); } }
function kokoroSay(x, next, run) {
  kokoroAt = { run, x, ms: 0, words: [], wi: -1 };
  kokoroTell({ say: x.say, voice: voicePick(), speed: LISTEN_SPEEDS[listenSpeed], id: run, ...(next ? { next: next.say } : {}) });
  listenTell();
}
function kokoroHush() { if (kokoroAt) { kokoroAt = null; kokoroTell({ stop: true }); } }
/** One of cue-speak's lines. */
function kokoroLine(v) {
  if (v.status) { Object.assign(kokoro, v.status); return renderMain(); }
  if (v.progress != null) { kokoro.progress = v.progress; return setProgress(); }
  // A voice got: it's the one Cue reads in now.
  if (v.got) { kokoro.getting = null; kokoro.ready = true; if (!kokoro.have.includes(v.got)) kokoro.have.push(v.got); pickVoice(v.got); kokoroStatus(true); return; }
  if (v.removed) { Object.assign(kokoro, { ready: false, have: [], mb: 0 }); pickVoice(""); return; }
  if (v.error && v.get) { kokoro.getting = null; toast(v.error); return renderMain(); }
  const a = kokoroAt, l = listening;
  if (v.playing != null && a?.run === v.playing) { a.ms = v.ms; a.words = v.words || []; kokoro.fails = 0; }
  else if (v.at != null && a?.run === v.at && a.ms && l) {
    // The word being heard: from when each starts (cue-speak's Words.swift, from Kokoro's own timings; the words
    // are the sentence's, split on spaces as here), else as far through its letters as through its sound.
    const words = [...a.x.say.matchAll(/\S+/g)], t = a.words;
    const wi = t.length === words.length ? Math.max(0, t.findLastIndex((ms) => ms <= v.ms))
      : Math.max(0, words.findLastIndex((m) => m.index <= (v.ms / a.ms) * a.x.say.length));
    if (wi === a.wi || !words[wi]) return;   // the same word: nothing moves
    a.wi = wi;
    l.w = [words[wi].index, words[wi][0].length];
    listenMarkNow(l);
  } else if (v.done != null && a?.run === v.done && v.end) { kokoroAt = null; if (l) { l.i++; listenSay(); } }
  else if (v.bye) kokoro.bye = true;
  // It retired after its 100 sentences (cue-speak): the sentence sent as it went, again to the fresh one.
  else if (v.gone && kokoro.bye) { kokoro.bye = false; if (a) kokoroSay(a.x, null, a.run); }
  else if (v.gone && kokoro.getting) { kokoro.getting = null; toast("The download stopped"); renderMain(); }
  else if (v.error || v.gone) {
    // Kokoro failing on a sentence: the Mac's voice says that one, and the natural voice the next. Three in a row:
    // the Mac's voice goes on from there.
    if (!a || (v.id != null && v.id !== a.run)) return;
    kokoroAt = null;
    if (++kokoro.fails >= 3) { kokoro.broke = true; toast("The natural voice stopped: going on in your Mac's voice"); }
    if (l) l.mac = l.i;
    if (l?.playing) listenSay();
  }
}
/** The voice Cue reads out in ("" the Mac's own). */
function pickVoice(id) {
  kokoro.broke = false;
  kokoro.fails = 0;
  invoke("set_setting", { key: "voice.pick", value: id }).catch((e) => toast(`Couldn't pick it: ${e}`));
  renderMain();   // the list closes now, not on the next redraw (none comes when it's the voice already in use)
}
/** Get a voice (and the engine, the first time). */
function getVoice(id) {
  kokoro.getting = id;
  kokoro.progress = 0;
  kokoroTell({ get: id });
  renderMain();
}
/** Kokoro's American English voices, as Settings lists them (each has a sample in ui/voices/). */
const KOKORO_VOICES = [["af_heart", "Heart", "woman"], ["af_bella", "Bella", "woman"], ["af_nicole", "Nicole", "woman"], ["af_sarah", "Sarah", "woman"],
  ["af_sky", "Sky", "woman"], ["af_alloy", "Alloy", "woman"], ["af_aoede", "Aoede", "woman"], ["af_jessica", "Jessica", "woman"], ["af_kore", "Kore", "woman"],
  ["af_nova", "Nova", "woman"], ["af_river", "River", "woman"], ["am_adam", "Adam", "man"], ["am_michael", "Michael", "man"], ["am_echo", "Echo", "man"],
  ["am_eric", "Eric", "man"], ["am_fenrir", "Fenrir", "man"], ["am_liam", "Liam", "man"], ["am_onyx", "Onyx", "man"], ["am_puck", "Puck", "man"]];
let voiceListOpen = false;
// The voice list closes on a click anywhere but the list and its button (before that click does its own thing).
document.addEventListener("click", (e) => { if (voiceListOpen && !e.target.closest?.(".vp-list, .vp-btn")) { voiceListOpen = false; renderMain(); } }, true);
/** The open voice list over the window, the sheet's edge included (a short sheet would cut it off): under its picker,
 *  as wide, as tall as fits down to the window's foot; over the picker when less than 160 px is left below. After
 *  each draw, and as the sheet scrolls under it. */
function placeVoiceList() {
  const list = document.querySelector(".vp-list"), btn = list && document.querySelector(".vp-btn");
  if (!btn) return;
  const r = btn.getBoundingClientRect(), below = innerHeight - r.bottom - 18, above = r.top - 18;
  const up = below < 160 && above > below, room = Math.min(324, up ? above : below);
  Object.assign(list.style, { left: `${r.left}px`, width: `${r.width}px`, maxHeight: `${room}px` });
  list.style.top = up ? `${r.top - 6 - Math.min(list.scrollHeight + 2, room)}px` : `${r.bottom + 6}px`;
}
document.addEventListener("scroll", (e) => { if (voiceListOpen && !e.target.matches?.(".vp-list")) placeVoiceList(); }, true);
addEventListener("resize", () => { if (voiceListOpen) placeVoiceList(); });
/** Settings → Voice: one Voice row, a picker showing the voice in use (▶ plays it); open, a short list: your Mac's
 *  voice, then Kokoro's women and men, one line each with ▶ and In use, ✓ (here: a click uses it) or ↓ 0.5 MB.
 *  Before the first one it says once that it brings the voice engine (125 MB); a download shows its bar. */
function voiceSettings() {
  kokoroStatus();
  const k = kokoro, pick = voicePick() && k.have.includes(voicePick()) ? voicePick() : "";
  const nameOf = (id) => (id ? KOKORO_VOICES.find(([v]) => v === id)?.[1] || id : "Your Mac's voice");
  const play = (id, name) => `<button class="vp-play" data-voice-sample="${id}" aria-label="Hear ${esc(name)}">${PLAY_ICON}</button>`;
  const btn = `<div class="vp-btn ${voiceListOpen ? "open" : ""}">${play(pick, nameOf(pick))}<button class="vp-open" data-voice-list aria-haspopup="listbox" aria-expanded="${voiceListOpen}"><span>${esc(nameOf(pick))}</span><span class="vp-caret">▾</span></button></div>`;
  const state = (id) => {
    if (k.getting === id) {
      const p = Math.round(k.progress * 100);
      return `<span class="vp-get"><span class="kk-bar"><span class="kk-fill" data-kokoro-progress style="width: ${p}%"></span></span><span class="dim" data-kokoro-pct>${k.ready ? `${p}%` : `${Math.round(k.progress * 125)} of 125 MB`}</span></span>`;
    }
    if (pick === id) return `<span class="vp-state">In use</span>`;
    if (!id || k.have.includes(id)) return `<button class="vp-use" data-voice-pick="${id}">${id ? "✓" : "Use"}</button>`;
    return `<button class="vp-down" data-voice-get="${id}" ${k.getting ? "disabled" : ""}>↓ ${k.ready ? "0.5 MB" : "Get"}</button>`;
  };
  const line = (id, name) => `<div class="vp-line ${pick === id ? "on" : ""}" role="option" aria-selected="${pick === id}" ${!id || k.have.includes(id) ? `data-voice-pick="${id}"` : ""}>${play(id, name)}<span class="vp-name">${esc(name)}</span>${state(id)}</div>`;
  const group = (who, title) => `<div class="vp-sec">${title}</div>${KOKORO_VOICES.filter(([, , w]) => w === who).map(([id, name]) => line(id, name)).join("")}`;
  const list = !voiceListOpen ? "" : `<div class="vp-list" role="listbox">
    <div class="vp-sec">Your Mac</div>${line("", "Your Mac's voice")}
    ${k.none ? `<div class="vp-intro">Natural voices need a Mac with Apple silicon.</div>`
      : `${k.ready ? "" : `<div class="vp-intro"><b>Natural voices</b>Kokoro, run on this Mac: nothing it reads leaves it. The first one brings the voice engine (125 MB, once); each after that is 0.5 MB. ▶ plays any of them first.</div>`}
    ${group("woman", "Natural · Women")}${group("man", "Natural · Men")}`}</div>`;
  const about = k.ready && k.have.length ? `<div class="vp-note">Natural voices (Kokoro) run on this Mac: nothing Cue reads out leaves it. ${k.mb} MB · <button class="vp-link" data-kokoro-remove>Remove</button></div>` : "";
  const risky = k.ready && k.risky ? `<div class="vp-note warn">macOS 26.4 and 26.5 have an Apple bug that can stop a natural voice now and then; Cue then goes on in your Mac's voice. macOS 26.6 fixes it.</div>` : "";
  return `<div class="set-group">Reading out loud</div>
    <div class="set-row vp-row"><div class="set-text"><div class="set-title">Voice</div><div class="set-sub">Listen reads replies out in it.</div>${about}${risky}</div><div class="vp">${btn}${list}</div></div>`;
}
let voiceSample = null;
/** ▶ beside a voice: its sample (shipped with Cue), again to stop it. */
function playSample(id) {
  const same = voiceSample?.id === id && !voiceSample.a.paused;
  voiceSample?.a.pause();
  voiceSample = null;
  if (window.speechSynthesis) speechSynthesis.cancel();
  if (same) return;
  // Your Mac's voice: said by it, the same words as the natural voices' samples.
  if (!id) { if (window.speechSynthesis) speechSynthesis.speak(new SpeechSynthesisUtterance("Step three is done and staged. Here's what changed, and what I'd check before you commit.")); return; }
  const a = new Audio(`voices/${id}.m4a`);
  voiceSample = { id, a };
  a.play().catch(() => toast("Couldn't play the sample"));
}
/** A download's bar, without redrawing Settings. */
function setProgress() {
  const p = Math.round((kokoro.progress || 0) * 100);
  document.querySelectorAll("[data-kokoro-progress]").forEach((el) => { el.style.width = `${p}%`; });
  document.querySelectorAll("[data-kokoro-pct]").forEach((el) => { el.textContent = kokoro.ready ? `${p}%` : `${Math.round((kokoro.progress || 0) * 125)} of 125 MB`; });
}
/** Its count and button on screen, without redrawing the chat. */
function listenTell() {
  const l = listening;
  document.querySelectorAll("[data-listen-of]").forEach((el) => { el.textContent = l ? `${Math.min(l.i + 1, l.n)} of ${l.n}` : ""; });
  document.querySelectorAll(".ls-fill").forEach((el) => { el.style.width = l ? `${Math.round((Math.min(l.i + 1, l.n) / Math.max(1, l.n)) * 100)}%` : "0"; });
}
/** Done with this reply: on to the next when it was started in the reader with "Then the next reply" (its session
 *  open or not: then the next of its replies as they were when it was last open). */
function listenNext() {
  const l = listening, cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
  if (sid === l.sid) l.all = readerReplies(cur.it, cur.s);
  // A file read in the reader ends there: the session's replies aren't what comes after it.
  const all = l.reader && listenThen && !l.key.startsWith("file:") ? l.all || [] : [];
  const next = all[all.findIndex((r) => r.key === l.key) + 1];
  if (!next) return listenStop();
  if (readerView?.key === l.key) readerView = { sid, key: next.key };
  listening = { ...l, key: next.key, i: 0, n: 0, html: md(stripStatus(next.text)), follow: true };
  renderMain();
  listenSay();
}
function listenPlay(sid, key, reader = false, from = 0) {
  if (!window.speechSynthesis && !kokoroOn()) return toast("This Mac can't read it out");
  openMsgs.add(key);   // a folded reply opens to be read
  const cur = current();
  listening = { sid, key, i: from, n: 0, playing: true, reader, all: cur ? readerReplies(cur.it, cur.s) : [], follow: true };
  invoke("music_hold").catch(() => {});   // what was playing (a video, music) pauses while it reads, and plays again at Stop
  renderMain();
  listenSay();
}
// Pause holds the sentence where it is (a natural voice's sound pauses, and Resume goes on from that word); the
// Mac's voice starts the sentence again on Resume, as Safari's speech can't be relied on to pause.
function listenPause() {
  if (!listening) return;
  if (kokoroAt) kokoroTell({ pause: true });
  else { listenRun++; speechSynthesis.cancel(); }
  listening.playing = false;
  renderMain();
}
function listenResume() {
  if (!listening) return;
  listening.playing = true;
  renderMain();
  if (kokoroAt) kokoroTell({ resume: true });
  else listenSay();
}
const listenToggle = () => (listening?.playing ? listenPause() : listenResume());
function listenSkip(dir) {
  if (!listening) return;
  listening.i = Math.max(0, Math.min(listening.n - 1, listening.i + dir));
  listening.w = null;
  listening.follow = true;
  if (listening.playing) return listenSay();
  kokoroHush();   // paused: Resume starts the sentence skipped to
  listenMarkNow(listening);
  listenTell();
}
function listenStop() {
  listenRun++;
  if (window.speechSynthesis) speechSynthesis.cancel();
  kokoroHush();
  const was = listening;
  listening = null;
  listenUnmark();
  if (was) invoke("music_release").catch(() => {});
  if (was) renderMain();
}
function listenFaster(n) {
  listenSpeed = n;
  try { localStorage.setItem("cue.listenRate", String(LISTEN_SPEEDS[n])); } catch {}
  if (listening?.playing) listenSay();
  else kokoroHush();   // paused: Resume says the sentence again at this speed
  renderMain();
}
function listenThenNext() {
  listenThen = !listenThen;
  try { localStorage.setItem("cue.listenThen", listenThen ? "1" : "0"); } catch {}
  renderMain();
}
/** A click in the reply being read, on a sentence (not a selection): it reads from there. */
function listenFrom(ev) {
  const l = listening, root = l && ev.target.closest(".msg");
  if (!root || root !== listenRoot(l.key) || !document.getSelection()?.isCollapsed) return false;
  const at = document.caretRangeFromPoint?.(ev.clientX, ev.clientY);
  const i = at ? listenSents(root).findIndex((x) => x.range?.isPointInRange(at.startContainer, at.startOffset)) : -1;
  if (i < 0) return false;
  l.i = i;
  l.follow = true;
  if (!l.playing) { l.playing = true; renderMain(); }
  listenSay();
  return true;
}
// The marks follow a redraw of the chat (and come back with its session).
function listenKeep(sid) {
  if (!listening || listening.sid !== sid) return;
  listenMarkNow(listening, true);
  listenTell();
}
const PLAY_ICON = `<svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path d="M7 4l13 8-13 8z" fill="currentColor"/></svg>`;
const PAUSE_ICON = `<svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path d="M6 4h4v16H6zM14 4h4v16h-4z" fill="currentColor"/></svg>`;
const BOOK_ICON = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 4h6a4 4 0 0 1 4 4v13a3 3 0 0 0-3-3H2zM22 4h-6a4 4 0 0 0-4 4v13a3 3 0 0 1 3-3h7z"/></svg>`;
/** Listen under a reply: the speaker and Listen; while it reads this one, ‖ Pause; paused on it, ▶ Resume (the
 *  same marks as the line above the box). Actions beside it are >_, so its ▶ isn't taken for one. */
function listenBtn(key) {
  const on = listening?.key === key;
  const [icon, label] = !on ? [SPEAKER_ICON, "Listen"] : listening.playing ? [PAUSE_ICON, "Pause"] : [PLAY_ICON, "Resume"];
  return `<button class="cv-pill ls-pill ${on ? "on" : ""}" data-listen="${esc(key)}">${icon}${label}</button>`;
}
/** Above the chat's box while a reply is read out: in its own session, how far; in another, whose (a click goes there); Skip code & tables, as in the reader. */
function listenLine(sid) {
  const l = listening;
  if (!l || !sid) return "";
  const where = l.key.startsWith("file:") ? `<button class="ls-where" data-view="read">${esc(l.key.split("/").pop())}</button>` : l.sid === sid ? `sentence <span data-listen-of="1">${Math.min(l.i + 1, l.n)} of ${l.n}</span>` : `<button class="ls-where" data-listen-open="${esc(l.sid)}">${esc(nameOf(l.sid, sessionOf(l.sid)?.project))}</button>`;
  return `<div class="ls-line" data-key="listen"><b>${l.playing ? "Listening" : "Paused"}</b><span class="dim"> · ${where}${listenSpeed ? ` · ${LISTEN_SPEEDS[listenSpeed]}×` : ""}</span><span class="grow"></span>
    ${listenPlainBox()}<button class="btn" data-listen-toggle="1" aria-label="${l.playing ? "Pause" : "Go on"}">${l.playing ? PAUSE_ICON : PLAY_ICON}</button><button class="btn" data-listen-stop="1">Stop</button></div>`;
}
/** The reader's foot while it reads the reply open there: a sentence back, pause, a sentence on, how far, speed,
 *  "Then the next reply", Stop. */
function listenPlayer(key) {
  const l = listening;
  if (!l || l.key !== key) return "";
  const of = Math.min(l.i + 1, l.n);
  return `<div class="foot ls-player">
    <span class="rd-seg"><button data-listen-skip="-1" aria-label="Back a sentence">«</button><button class="ls-pp" data-listen-toggle="1" aria-label="${l.playing ? "Pause" : "Go on"}">${l.playing ? PAUSE_ICON : PLAY_ICON}</button><button data-listen-skip="1" aria-label="Next sentence">»</button></span>
    <span class="dim ls-of" data-listen-of="1">${of} of ${l.n}</span>
    <span class="ls-track"><span class="ls-fill" style="width: ${Math.round((of / Math.max(1, l.n)) * 100)}%"></span></span>
    <span class="rd-seg ls-speeds">${LISTEN_SPEEDS.map((x, n) => `<button class="${n === listenSpeed ? "on" : ""}" data-listen-speed="${n}">${x}×</button>`).join("")}</span>
    <span class="ls-opts">${key.startsWith("file:") ? "" : `<label class="ls-then"><input type="checkbox" data-listen-then="1" ${listenThen ? "checked" : ""}/>Then the next reply</label>`}
    <button class="btn" data-listen-stop="1">Stop</button></span></div>`;
}
