// Run: node tests/box.test.js  (the one message box: ui/box/markup.js, send.js, end.js)
// The box's markup is checked against what the builders drew before they moved onto it (chat/compose.js box, before 3b;
// desk/view.js deskBox and deskItemFoot, desk/act.js deskBackHtml, desk/page.js deskGoBackHtml, before 3c;
// mini.js's answer box, chat/send.js btwPanel's and sheets.js forwardPop's, before 3d), less their hover text, so moving a
// box onto it changes nothing you see. Its send, against a fake invoke.
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const esc = (s) => String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const calls = [], toasts = [];
let draws = 0, saves = 0, answer = async () => "sent via tmux";
const ctx = {
  esc,
  MIC_ICON: "<mic/>", IMAGE_ICON: "<img/>", CLOCK_ICON: "<clock/>", DESK_UNDO_ICON: "<undo/>",
  state: { restarting: {} },
  sessions: {},
  sessionOf: (sid) => ctx.sessions[sid] || null,
  invoke: (cmd, args) => { calls.push([cmd, args]); return answer(cmd, args); },
  toast: (t) => toasts.push(t),
  renderMain: () => { draws++; },
  saveDrafts: () => { saves++; },
  insSent() {},
  suggestionFor: () => "",
  dictating: null,
  drafts: {},
  CSS: { escape: (s) => s },
  document: { addEventListener() {}, querySelector: () => null },
  requestAnimationFrame: (f) => setTimeout(f, 0),
  setTimeout, Date, Promise, JSON, Map, Set,
};
ctx.draft = (id) => (ctx.drafts[id] ||= { text: "", choices: {}, images: [] });
vm.createContext(ctx);
// In the windows' order (index.html): desk/act.js puts its boxes on box/send.js BOX_OWNERS as it loads.
for (const f of ["box/markup.js", "box/send.js", "box/end.js", "chat/send.js", "chat/compose.js", "desk/clicks.js", "desk/page.js", "desk/away.js", "desk/view.js", "desk/act.js", "sheets.js"]) {
  vm.runInContext(fs.readFileSync(`${__dirname}/../ui/${f}`, "utf8").replace(/^(const|let) /gm, "var "), ctx, { filename: f });
}
// What the builders call that isn't theirs to draw.
Object.assign(ctx, {
  cmdMenu: (k) => `<cmd ${k}>`, slashHint: (k) => `<hint ${k}>`, quickRow: (a) => `<quick ${a}>`, now: () => 1000,
  // Today's Desk: the new layout's box (desk/view.js deskCBox) isn't one of the builders checked here.
  deskCOn: () => false,
  deskBoxes: new Map(), deskSelection: () => "P", deskLeadName: () => "lead-1", deskLead: () => ({ sid: "L" }), deskLeadEnded: () => false, leadS: { session_id: "L", state: "waiting" }, deskLeadSession: () => ctx.leadS, deskOf: () => ({ project: "P" }), listenLine: (sid) => `<ls ${sid}>`,
  deskItem: (p, id) => ctx.items[id], items: {}, deskHolderSid: (it) => it?.holder?.sid || "", deskPickOptions: () => [],
  deskNum: (i) => `Task #${i.id}`, deskShort: (t) => t.split(" ").slice(0, 3).join(" "), deskItemRequest: () => null, deskHolderName: () => "holder",
});
// What the window's code returns is from another realm (vm): compared as plain data.
const plain = (x) => (x === undefined ? x : JSON.parse(JSON.stringify(x)));
const eq = (a, b, why) => assert.deepStrictEqual(plain(a), plain(b), why);
const noTitle = (html) => html.replace(/ title="[^"]*"/g, "");
const D = (key, text, images = []) => { ctx.drafts[key] = { text, choices: {}, images }; };
const PIC = { name: "a.png", mime: "image/png", data: "data:image/png;base64,AA", path: "/tmp/a.png" };

// ---- markup: a session's chat box (chat/compose.js box, on boxHtml since step 3b) ----
// What box() drew before it moved onto boxHtml, less its hover text, word for word.
const OLD_COMPOSER = [
  "<div class=\"composer\"><div class=\"field drop \" data-drop=\"s:A\"><div class=\"cmd-slot\"><cmd s:A></div><quick data-quick=\"s:A\"><div class=\"chips\"><span class=\"chip\"><button class=\"chip-img\" data-draft-lightbox=\"s:A:0\" aria-label=\"Open a.png\"><img src=\"data:image/png;base64,AA\" alt=\"a.png\"/></button><button class=\"chip-x\" data-unattach=\"s:A:0\" aria-label=\"Remove image\">×</button></span></div><div class=\"field-row\">\n    <div class=\"ta-wrap\"><textarea rows=\"2\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\" data-text=\"s:A\" placeholder=\"Reply…\">hello &lt;b&gt;</textarea><div class=\"sug-line\"  aria-hidden=\"true\"></div></div>\n    <button class=\"icon-btn mic \" data-mic=\"s:A\" aria-label=\"Dictate\"><mic/></button>\n    <button class=\"icon-btn\" data-attach=\"s:A\" aria-label=\"Add image\"><img/></button>\n    <button class=\"icon-btn \" data-send-at-open=\"s:A\" aria-label=\"Send at a time\"><clock/></button>\n    <x/><alt/><button class=\"btn primary send\" data-send=\"A\" >Send</button></div><hint s:A></div></div>",
  "<div class=\"composer\"><div class=\"field drop \" data-drop=\"s:A\"><div class=\"cmd-slot\"><cmd s:A></div><div class=\"field-row\">\n    <div class=\"ta-wrap\"><textarea rows=\"2\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\" data-text=\"s:A\" class=\"sug\" aria-label=\"Suggested: Go on. Tab types it in.\" placeholder=\" \"></textarea><div class=\"sug-line\"  aria-hidden=\"true\"><span class=\"sug-k\">Suggested</span><span class=\"sug-t\">Go on</span><kbd>Tab</kbd></div></div>\n    <button class=\"icon-btn mic on\" data-mic=\"s:A\" aria-label=\"Dictate\"><mic/></button>\n    <button class=\"icon-btn\" data-attach=\"s:A\" aria-label=\"Add image\"><img/></button>\n    <button class=\"icon-btn on\" data-send-at-open=\"s:A\" aria-label=\"Send at a time\"><clock/></button>\n    <button class=\"btn primary send\" data-send=\"A\" disabled>Send</button></div><div class=\"send-at-bar\">Send at <input type=\"time\" data-send-at-time value=\"08:01\"><span class=\"grow\"></span><button class=\"btn sm\" data-send-at-x>Cancel</button><button class=\"btn sm primary\" data-send-at-go=\"s:A\">Send ${WHEN}</button></div><hint s:A></div></div>",
  "<div class=\"composer\"><div class=\"field  bang\" data-drop=\"\"><div class=\"field-row\">\n    <div class=\"ta-wrap\"><textarea rows=\"2\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\" data-text=\"new\" placeholder=\"What to do\">! ls</textarea><div class=\"sug-line\"  aria-hidden=\"true\"></div></div>\n    <button class=\"icon-btn mic \" data-mic=\"new\" aria-label=\"Dictate\"><mic/></button>\n    \n    \n    <button class=\"btn primary send\" data-new >Run</button></div><hint new></div></div>",
];
D("s:A", "hello <b>", [PIC]);
assert.strictEqual(ctx.box("s:A", "Reply…", "Send", 'data-send="A"', true, "<alt/>", "<x/>"), OLD_COMPOSER[0], "a session's box with words and a picture");
ctx.dictating = { key: "s:A" };
ctx.sendAtFor = "s:A";
ctx.suggestionFor = () => "Go on";
D("s:A", "");
assert.strictEqual(ctx.box("s:A", "Reply…", "Send", 'data-send="A"', true, "<alt/>", "", false), OLD_COMPOSER[1].replace("${WHEN}", ctx.sendAtWhen("08:01").words), "empty, a suggestion, dictating, the Send at bar open, no quick phrases");
D("new", "! ls");
assert.strictEqual(ctx.box("new", "What to do", "Start", "data-new", false), OLD_COMPOSER[2], "a box without pictures, a \"!\" line");
assert.doesNotMatch(ctx.box("s:A", "R", "Send", "", true), /title=/, "no hover text");
ctx.dictating = null; ctx.sendAtFor = null; ctx.suggestionFor = () => "";

// ---- markup: the Desk's boxes (on boxHtml since step 3c) ----
// What desk/view.js deskBox and deskItemFoot, desk/act.js deskBackHtml and desk/page.js deskGoBackHtml drew before they
// moved onto boxHtml, word for word.
const OLD_DESK = {
  item: "<div class=\"dk-box\" data-drop=\"desk:[&quot;P&quot;,7]\"><quick data-quick=\"desk:[&quot;P&quot;,7]\"><span class=\"dk-pre\" data-desk-pre=\"\">To lead-1 · Task #7</span><div class=\"dk-box-row\"><div class=\"ta-wrap\"><textarea rows=\"1\" data-text=\"desk:[&quot;P&quot;,7]\" placeholder=\"Remove the actions…\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\">why &amp; how</textarea><div class=\"sug-line\"  aria-hidden=\"true\"></div></div><button class=\"icon-btn mic \" data-mic=\"desk:[&quot;P&quot;,7]\" aria-label=\"Dictate\"><mic/></button><button class=\"icon-btn\" data-attach=\"desk:[&quot;P&quot;,7]\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-send=\"desk:[&quot;P&quot;,7]\">Send</button></div></div>",
  itemSug: "<div class=\"dk-box\" data-drop=\"desk:[&quot;P&quot;,7]\"><quick data-quick=\"desk:[&quot;P&quot;,7]\"><span class=\"dk-pre\" data-desk-pre=\"\">To lead-1 · Task #7</span><div class=\"dk-box-row\"><div class=\"ta-wrap\"><textarea rows=\"1\" data-text=\"desk:[&quot;P&quot;,7]\" class=\"sug\" data-sug-sid=\"item:P|7\" aria-label=\"Suggested: Looks good. Tab types it in.\" placeholder=\" \" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\"></textarea><div class=\"sug-line\"  aria-hidden=\"true\"><span class=\"sug-k\">Suggested</span><span class=\"sug-t\">Looks good</span><kbd>Tab</kbd></div></div><button class=\"icon-btn mic \" data-mic=\"desk:[&quot;P&quot;,7]\" aria-label=\"Dictate\"><mic/></button><button class=\"icon-btn\" data-attach=\"desk:[&quot;P&quot;,7]\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-send=\"desk:[&quot;P&quot;,7]\">Send</button></div></div>",
  itemCrew: "<div class=\"dk-box\" data-drop=\"desk:[&quot;P&quot;,8]\"><quick data-quick=\"desk:[&quot;P&quot;,8]\"><span class=\"dk-pre\" data-desk-pre=\"\">To mate-1, through holder · sent as #8</span><div class=\"dk-box-row\"><div class=\"ta-wrap\"><textarea rows=\"1\" data-text=\"desk:[&quot;P&quot;,8]\" placeholder=\"Write to it here…\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\"></textarea><div class=\"sug-line\"  aria-hidden=\"true\"></div></div><button class=\"icon-btn mic \" data-mic=\"desk:[&quot;P&quot;,8]\" aria-label=\"Dictate\"><mic/></button><button class=\"icon-btn\" data-attach=\"desk:[&quot;P&quot;,8]\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-send=\"desk:[&quot;P&quot;,8]\">Send</button></div></div>",
  holder: "<div class=\"dk-box\" data-drop=\"desk-to:[&quot;H&quot;,&quot;&quot;]\"><quick data-quick=\"desk-to:[&quot;H&quot;,&quot;&quot;]\"><div class=\"dk-box-row\"><div class=\"ta-wrap\"><textarea rows=\"1\" data-text=\"desk-to:[&quot;H&quot;,&quot;&quot;]\" class=\"sug\" data-sug-sid=\"H\" aria-label=\"Suggested: Yes. Tab types it in.\" placeholder=\" \" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\"></textarea><div class=\"sug-line\"  aria-hidden=\"true\"><span class=\"sug-k\">Suggested</span><span class=\"sug-t\">Yes</span><kbd>Tab</kbd></div></div><button class=\"icon-btn mic \" data-mic=\"desk-to:[&quot;H&quot;,&quot;&quot;]\" aria-label=\"Dictate\"><mic/></button><button class=\"icon-btn\" data-attach=\"desk-to:[&quot;H&quot;,&quot;&quot;]\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-send=\"desk-to:[&quot;H&quot;,&quot;&quot;]\">Send</button></div></div>",
  itemMocks: "<div class=\"dk-box\" data-drop=\"desk:[&quot;P&quot;,9]\"><quick data-quick=\"desk:[&quot;P&quot;,9]\"><span class=\"dk-pre\" data-desk-pre=\"\">To cue-9 · Task #9</span><div class=\"dk-box-row\"><div class=\"ta-wrap\"><textarea rows=\"1\" data-text=\"desk:[&quot;P&quot;,9]\" placeholder=\"Comments… (Pick an option above to go ahead)\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\"></textarea><div class=\"sug-line\"  aria-hidden=\"true\"></div></div><button class=\"icon-btn mic \" data-mic=\"desk:[&quot;P&quot;,9]\" aria-label=\"Dictate\"><mic/></button><button class=\"icon-btn\" data-attach=\"desk:[&quot;P&quot;,9]\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-send=\"desk:[&quot;P&quot;,9]\">Send</button></div></div>",
  foot: "<div class=\"dk-box\" data-drop=\"desk:[&quot;P&quot;,7]\"><span class=\"dk-pre\" data-desk-pre=\"\">To cue-7 · Task #7</span><div class=\"chips\"><span class=\"chip\"><button class=\"chip-img\" data-draft-lightbox=\"desk:[&quot;P&quot;,7]:0\" aria-label=\"Open a.png\"><img src=\"data:image/png;base64,AA\" alt=\"a.png\"/></button><button class=\"chip-x\" data-unattach=\"desk:[&quot;P&quot;,7]:0\" aria-label=\"Remove image\">×</button></span></div><div class=\"dk-box-row\"><textarea rows=\"1\" data-text=\"desk:[&quot;P&quot;,7]\" placeholder=\"Remove the actions…\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\">yes</textarea><button class=\"icon-btn mic on\" data-mic=\"desk:[&quot;P&quot;,7]\" aria-label=\"Stop dictating\"><mic/></button><button class=\"icon-btn\" data-attach=\"desk:[&quot;P&quot;,7]\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-send=\"desk:[&quot;P&quot;,7]\">Send</button></div></div>",
  footCrew: "<div class=\"dk-box\" data-drop=\"desk:[&quot;P&quot;,8]\"><span class=\"dk-pre\" data-desk-pre=\"\">To mate-1 · Task #8</span><div class=\"dk-box-row\"><textarea rows=\"1\" data-text=\"desk:[&quot;P&quot;,8]\" placeholder=\"Fix the thing…\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\"></textarea><button class=\"icon-btn mic \" data-mic=\"desk:[&quot;P&quot;,8]\" aria-label=\"Dictate\"><mic/></button><button class=\"icon-btn\" data-attach=\"desk:[&quot;P&quot;,8]\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-send=\"desk:[&quot;P&quot;,8]\">Send</button></div></div>",
  back: "<div class=\"dk-sb\"><div class=\"dk-sb-h\"><span class=\"dk-sb-l\">Why send it back?</span><button class=\"dk-sb-x\" data-desk-act=\"back_cancel\" data-id=\"7\">Cancel</button></div><div class=\"dk-box dk-sb-box\" data-drop=\"back:7\"><div class=\"chips\"><span class=\"chip\"><button class=\"chip-img\" data-draft-lightbox=\"back:7:0\" aria-label=\"Open a.png\"><img src=\"data:image/png;base64,AA\" alt=\"a.png\"/></button><button class=\"chip-x\" data-unattach=\"back:7:0\" aria-label=\"Remove image\">×</button></span></div><div class=\"dk-box-row\"><textarea rows=\"1\" data-text=\"back:7\" aria-label=\"Why send it back?\" placeholder=\"What isn't right yet\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\">not yet</textarea><button class=\"icon-btn mic \" data-mic=\"back:7\" aria-label=\"Dictate\"><mic/></button><button class=\"icon-btn\" data-attach=\"back:7\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-act=\"back_go\" data-id=\"7\">Send</button></div></div></div>",
  backMic: "<div class=\"dk-sb\"><div class=\"dk-sb-h\"><span class=\"dk-sb-l\">Why send it back?</span><button class=\"dk-sb-x\" data-desk-act=\"back_cancel\" data-id=\"7\">Cancel</button></div><div class=\"dk-box dk-sb-box\" data-drop=\"back:7\"><div class=\"dk-box-row\"><textarea rows=\"1\" data-text=\"back:7\" aria-label=\"Why send it back?\" placeholder=\"What isn't right yet\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\"></textarea><button class=\"icon-btn mic on\" data-mic=\"back:7\" aria-label=\"Stop dictating\"><mic/></button><button class=\"icon-btn\" data-attach=\"back:7\" aria-label=\"Add an image\"><img/></button><button class=\"btn primary small\" data-desk-act=\"back_go\" data-id=\"7\">Send</button></div></div></div>",
  goback: "<div class=\"dk-sb dk-gb-box\"><div class=\"dk-sb-h\"><span class=\"dk-sb-l\">What should happen instead?</span><button class=\"dk-sb-x\" data-desk-go-back=\"cancel\">Cancel</button></div><div class=\"dk-box dk-sb-box\"><div class=\"dk-box-row\"><textarea rows=\"2\" data-text=\"goback\" aria-label=\"What should happen instead?\" placeholder=\"Your answer goes to each one with its decision\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\">undo &lt;it&gt;</textarea><button class=\"btn primary small\" data-desk-go-back=\"send\">Send</button></div></div></div>",
  gobackShut: "<div class=\"dk-gb\"><span class=\"dk-gb-n\">1 ticked</span><span class=\"grow\"></span><button class=\"btn small\" data-desk-go-back=\"clear\">Clear</button><button class=\"btn primary small\" data-desk-go-back=\"dec\">Go back on this</button></div>"
};
const deskD = { project: "P", items: [] };
const same = (name, html) => assert.strictEqual(html, OLD_DESK[name], name);
// The lead's box: a suggestion and a picture; then words, dictating.
ctx.deskOpen = null;
ctx.suggestionFor = (sid) => (sid === "L" ? "Ship it" : "");
let key = ctx.deskBoxKey({ project: "P" });
D(key, "", [PIC]);
// It is a session's chat box, the lead's (#688): Listen's line, then the Board's composer in its foot; Queue while it works.
const leadBox = (ph, label) => `<ls L><div class="foot">${ctx.box(key, ph, label, `data-desk-send="${esc(key)}"`, true)}</div>`;
let html = ctx.deskBox(deskD);
assert.strictEqual(html, leadBox("Tell lead-1: a bug, a feature, or a question…", "Send"), "lead");
assert.ok(html.includes('<div class="composer">') && html.includes('rows="2"') && html.includes('data-sug-sid="L"') && html.includes("Ship it") && html.includes("data-draft-lightbox"), "lead: the composer, the lead's suggestion, the picture");
ctx.suggestionFor = () => ""; ctx.dictating = { key }; D(key, 'a <b> & "c"');
assert.strictEqual(ctx.deskBox(deskD), leadBox("Tell lead-1: a bug, a feature, or a question…", "Send"), "leadMic");
ctx.dictating = null;
ctx.leadS = { session_id: "L", state: "working" };
assert.strictEqual(ctx.deskBox(deskD), leadBox("Message lead-1… Cue keeps it and sends it when this turn ends", "Queue"), "lead at work: Queue");
ctx.leadS = { session_id: "L", state: "waiting" };
// An item's box, to the lead: the "To …" line over it; then Claude's suggestion for the item.
ctx.items[7] = { id: 7, title: "Remove the actions row", state: "doing" };
ctx.deskOpen = { kind: "item", id: 7 };
key = ctx.deskBoxKey({ project: "P", item: 7 });
D(key, "why & how");
same("item", ctx.deskBox(deskD));
ctx.suggestionFor = (sid) => (sid === "item:P|7" ? "Looks good" : ""); D(key, "");
same("itemSug", ctx.deskBox(deskD));
ctx.suggestionFor = () => "";
// An item a live crew member holds: to it, through its lead.
ctx.sessions.H = { session_id: "H" };
ctx.items[8] = { id: 8, title: "Fix the thing now", state: "doing", holder: { kind: "crew", sid: "H", lead_sid: "H", name: "mate-1" } };
ctx.deskOpen = { kind: "item", id: 8 };
same("itemCrew", ctx.deskBox(deskD));
// A holder's chat open, its suggestion.
ctx.deskOpen = { kind: "holder", sid: "H" }; ctx.suggestionFor = (sid) => (sid === "H" ? "Yes" : "");
same("holder", ctx.deskBox(deskD));
ctx.suggestionFor = () => "";
// An item to check with mocks to pick from.
ctx.items[9] = { id: 9, title: "Make the new mocks", state: "to_check", holder: { kind: "session", sid: "H", name: "cue-9" } };
ctx.deskOpen = { kind: "item", id: 9 }; ctx.deskPickOptions = () => [1];
same("itemMocks", ctx.deskBox(deskD));
ctx.deskPickOptions = () => [];
// An item that asks you, its holder's box (deskItemFoot): dictating, a picture; a crew member's, empty.
const ask = { id: 7, title: "Remove the actions row", state: "needs_you", holder: { kind: "session", sid: "H", name: "cue-7" } };
key = ctx.deskBoxKey({ project: "P", sid: "H", item: 7 });
ctx.dictating = { key }; D(key, "yes", [PIC]);
same("foot", ctx.deskItemFoot(ask, { project: "P" }));
ctx.dictating = null;
same("footCrew", ctx.deskItemFoot({ id: 8, title: "Fix the thing now", state: "needs_you", holder: { kind: "crew", lead_sid: "H", sid: "H", name: "mate-1" } }, { project: "P" }));
// Send back's box (deskBackHtml): a picture; empty, dictating.
D("back:7", "not yet", [PIC]);
same("back", ctx.deskBackHtml(7));
ctx.dictating = { key: "back:7" }; D("back:7", "");
same("backMic", ctx.deskBackHtml(7));
ctx.dictating = null;
// Go back on ticked decisions (deskGoBackHtml): open (no pictures, no mic), and shut.
ctx.deskGoBackOpen = "dec"; D("goback", "undo <it>");
same("goback", ctx.deskGoBackHtml("dec", 2));
ctx.deskGoBackOpen = null;
same("gobackShut", ctx.deskGoBackHtml("dec", 1));
for (const html of Object.values(OLD_DESK)) assert.doesNotMatch(html, /title=/, "no hover text");

// ---- markup: the mini panel's box, btw's and a forward's note (on boxHtml since step 3d) ----
// What mini.js deskAskHtml's answer box, chat/send.js btwPanel and sheets.js
// forwardPop drew before they moved onto boxHtml, word for word.
const OLD_3D = {
  btw: "<div class=\"btw-pop\">\n    <div class=\"btw-head\">BY THE WAY · NOT PART OF THE CHAT<button class=\"btw-clear\" data-act=\"btw-clear\" data-sid=\"A\">Clear</button><button class=\"btw-x\" data-act=\"btw-close\" aria-label=\"Close\">×</button></div>\n    <div class=\"btw-ask\"><textarea data-text=\"btw:A\" rows=\"1\" placeholder=\"Ask without interrupting it…  ↵\" spellcheck=\"false\">why &lt;so&gt;?</textarea></div>\n    <div class=\"btw-body\"><div class=\"btw-q\">third</div><div class=\"btw-a dim\"><span class=\"dot-live\"></span>Thinking…</div><div class=\"btw-q\">second</div><div class=\"btw-a bad\">boom</div><div class=\"btw-q\">first</div><div class=\"btw-a msg\"><md>an answer</md></div><div class=\"btw-a-acts\"><button class=\"btw-fork\" data-act=\"btw-fork\" data-sid=\"A\" data-i=\"0\" >Fork</button></div></div></div>",
  btwLead: "<div class=\"btw-pop\">\n    <div class=\"btw-head\">BY THE WAY · ASKS THE LEAD, NOT PART OF ITS CHAT<button class=\"btw-x\" data-act=\"btw-close\" aria-label=\"Close\">×</button></div>\n    <div class=\"btw-ask\"><textarea data-text=\"btw:A\" rows=\"1\" placeholder=\"Ask the lead without interrupting it…  ↵\" spellcheck=\"false\"></textarea></div>\n    <div class=\"btw-body\"><div class=\"dim btw-empty\">A copy of the lead's conversation answers, so neither the lead nor its crew sees it. Each question costs about one turn.</div></div></div>",
  fwd: "<div class=\"fwd-catch\" data-act=\"cancel-forward\"></div><div class=\"fwd-pop\" role=\"dialog\" aria-label=\"Send to another session\">\n    <div class=\"fwd-head\">SEND THIS MESSAGE TO</div>\n    <div class=\"fwd-list\"><button class=\"fwd-opt on\" data-fwd-to=\"A\">\n      <badge claude><span class=\"proj\">proj&lt;1&gt;</span><span class=\"grow\"></span><span class=\"fwd-st\">working</span></button></div>\n    <textarea rows=\"2\" class=\"fwd-note\" spellcheck=\"false\" data-text=\"fwd\" placeholder=\"Add a note (optional)\">a &lt;note&gt;</textarea>\n    <div class=\"fwd-foot\"><button class=\"btn\" data-act=\"cancel-forward\">Cancel</button><button class=\"btn primary\" data-act=\"send-forward\" >Send to proj&lt;1&gt;</button></div></div>",
  fwdNone: "<div class=\"fwd-catch\" data-act=\"cancel-forward\"></div><div class=\"fwd-pop\" role=\"dialog\" aria-label=\"Send to another session\">\n    <div class=\"fwd-head\">SEND THIS MESSAGE TO</div>\n    <div class=\"fwd-list\"><button class=\"fwd-opt \" data-fwd-to=\"A\">\n      <badge claude><span class=\"proj\">proj&lt;1&gt;</span><span class=\"grow\"></span><span class=\"fwd-st\">working</span></button></div>\n    <textarea rows=\"2\" class=\"fwd-note\" spellcheck=\"false\" data-text=\"fwd\" placeholder=\"Add a note (optional)\"></textarea>\n    <div class=\"fwd-foot\"><button class=\"btn\" data-act=\"cancel-forward\">Cancel</button><button class=\"btn primary\" data-act=\"send-forward\" disabled>Pick a session</button></div></div>",
  mini: "<div class=\"m-dk-box\"><textarea rows=\"1\" data-text=\"ans:7\" data-mdk-box=\"7\" aria-label=\"Your answer\" placeholder=\"Your answer\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\">yes &lt;please&gt;</textarea><button class=\"btn primary small\" data-mdk-send=\"7\">Send</button></div>",
  miniEmpty: "<div class=\"m-dk-box\"><textarea rows=\"1\" data-text=\"ans:7\" data-mdk-box=\"7&quot;x\" aria-label=\"Your answer\" placeholder=\"Your answer\" spellcheck=\"false\" autocorrect=\"off\" autocapitalize=\"off\" autocomplete=\"off\"></textarea><button class=\"btn primary small\" data-mdk-send=\"7&quot;x\">Send</button></div>"
};
const same3 = (name, html) => assert.strictEqual(html, OLD_3D[name], name);
Object.assign(ctx, { phrases: () => ["Yes", "No <way>"], md: (t) => `<md>${esc(t)}</md>`, badge: (h) => `<badge ${h}>`, idleNote: () => false, crewPageOn: () => false });
// btw's panel: asked, failed and answered; then the lead's, empty.
ctx.btwFor = "A"; D("btw:A", "why <so>?");
ctx.btwLog.set("A", [{ q: "first", a: "an answer", err: null }, { q: "second", a: null, err: "boom" }, { q: "third", a: null, err: null }]);
same3("btw", ctx.btwPanel("A"));
ctx.crewPageOn = () => true; ctx.btwLog.delete("A"); D("btw:A", "");
same3("btwLead", ctx.btwPanel("A"));
ctx.crewPageOn = () => false; ctx.btwFor = null;
// Forward's menu: a note, a session picked; then none picked.
const was3 = { items: ctx.state.items, sessions: ctx.state.sessions }, sessionOf3 = ctx.sessionOf;
ctx.sessionOf = (sid) => ctx.state.sessions.find((s) => s.session_id === sid) || null;
ctx.state.items = []; ctx.state.sessions = [{ session_id: "A", project: "proj<1>", harness: "claude" }, { session_id: "X", project: "from", harness: "claude" }];
ctx.forward = { fromSid: "X", to: "A", key: "k", from: "from", text: "t" }; D("fwd", "a <note>");
same3("fwd", ctx.forwardPop());
ctx.forward = { fromSid: "X", to: null, key: "k" }; D("fwd", "");
same3("fwdNone", ctx.forwardPop());
Object.assign(ctx.state, was3); ctx.sessionOf = sessionOf3; ctx.forward = null;
// The mini panel's answer box, as mini.js deskAskHtml asks for it (that's inside the mini window's own block).
const miniBox = (key, i) => ctx.boxHtml({ look: "mini", key, attrs: `data-mdk-box="${esc(i.id)}"`, label: "Your answer", placeholder: "Your answer", send: { attrs: `data-mdk-send="${esc(i.id)}"` } });
D("ans:7", "yes <please>");
same3("mini", miniBox("ans:7", { id: "7" }));
D("ans:7", "");
same3("miniEmpty", miniBox("ans:7", { id: '7"x' }));
assert.match(fs.readFileSync(`${__dirname}/../ui/mini.js`, "utf8"), /const box = key \? boxHtml\(\{ look: "mini", key, attrs: `data-mdk-box="\$\{esc\(i\.id\)\}"`, label: "Your answer", placeholder: "Your answer", send: \{ attrs: `data-mdk-send="\$\{esc\(i\.id\)\}"` \} \}\) : "";/, "mini.js asks for that box");
for (const html of Object.values(OLD_3D)) assert.doesNotMatch(html, /title=/, "no hover text");

// ---- putting words back ----
assert.strictEqual(ctx.boxMerge("sent", ""), "sent");
assert.strictEqual(ctx.boxMerge("sent", "  \n"), "sent", "a box of spaces counts as empty");
assert.strictEqual(ctx.boxMerge("sent", "typed since"), "sent\n\ntyped since");
D("k", "typed since", [{ name: "b.png" }]);
saves = draws = 0;
let d = ctx.boxPutBack("k", "sent", [{ name: "a.png" }]);
eq([d.text, d.images.map((i) => i.name), saves, draws], ["sent\n\ntyped since", ["a.png", "b.png"], 1, 1], "ahead of what's there, saved and drawn");
d = ctx.boxPutBack("k2", "held", [{ name: "a.png" }], { imagesLast: true, save: false, draw: false });
eq([d.text, d.images.length, saves, draws], ["held", 1, 1, 1], "into an empty box, neither saved nor drawn");
let focused = "";
ctx.boxPutBack("k", "again", [], { focus: (k) => { focused = k; } });
assert.strictEqual(focused, "k", "a focus function gets the key");
const pic = { name: "b.png" }; D("k3", "", [pic]);
ctx.boxPutBack("k3", "x", [{ name: "a.png" }], { imagesLast: true });
eq(ctx.drafts.k3.images.map((i) => i.name), ["b.png", "a.png"], "imagesLast: after the box's own (pullHeld)");

// ---- the end of a chat ----
const el = (scrollHeight, scrollTop, clientHeight) => ({ scrollHeight, scrollTop, clientHeight, isConnected: true });
assert.strictEqual(ctx.boxAtEnd(el(1000, 600, 400)), true, "at the end");
assert.strictEqual(ctx.boxAtEnd(el(1000, 577, 400)), true, "23px short is at the end");
assert.strictEqual(ctx.boxAtEnd(el(1000, 576, 400)), false, "24px short isn't");
assert.strictEqual(ctx.boxAtEnd(el(1000, 590, 400), 4), false, "a tighter test");
assert.strictEqual(ctx.boxAtEnd(null), false, "no chat");
const c = el(1000, 0, 400);
ctx.boxStickToEnd(c, false);
assert.strictEqual(c.scrollTop, 1000, "to its end");
assert.strictEqual(c._pinTop, 1000);

// ---- sending ----
(async () => {
  const reset = () => { calls.length = 0; toasts.length = 0; draws = saves = 0; answer = async () => "sent via tmux"; };
  const toS = (sid, o) => ({ call: ctx.boxToSession(sid, o) });
  // The guards: nothing goes, the box is kept.
  reset();
  ctx.sessions.S = { session_id: "S", state: "working" };
  D("s:S", "hi");
  ctx.state.restarting.S = true;
  eq(await ctx.boxSend({ sid: "S" }, "s:S", toS("S")), { held: "It's restarting. Send once it's back." });
  delete ctx.state.restarting.S;
  ctx.sessions.S.state = "deciding";
  eq(await ctx.boxSend({ sid: "S" }, "s:S", toS("S")), { held: "It's asking you something first. Answer that, then send." });
  eq([calls.length, toasts.length, ctx.drafts["s:S"].text], [0, 2, "hi"], "each said why; nothing sent; the box kept");
  assert.strictEqual((await ctx.boxSend({}, "s:S", { guardSid: "S", call: async () => "" })).held !== undefined, true, "guardSid: the Desk's holder");
  assert.strictEqual((await ctx.boxSend({ sid: "S" }, "s:S", { guard: false, call: async () => "ok" })).ok, true, "guard: false (a reply to a card)");
  ctx.sessions.S.state = "working";

  // Sent: pictures cut down to name, mime and data; the box emptied before it goes; about and now.
  reset();
  D("s:S", "  look  ", [PIC]);
  let seen = null;
  const r = await ctx.boxSend({ sid: "S" }, "s:S", { call: (t, im) => { seen = ctx.drafts["s:S"]; return ctx.boxToSession("S", { about: "About #7 (x): ", now: true })(t, im); } });
  eq(r, { ok: true, result: "sent via tmux" });
  assert.strictEqual(seen, undefined, "the box is empty while it goes");
  eq(calls, [["send_to_session", { sessionId: "S", text: "About #7 (x): look", images: [{ name: PIC.name, mime: PIC.mime, data: PIC.data }], now: true }]]);
  assert.strictEqual(draws, 1, "drawn once before it goes");

  // Didn't go: back in the box, ahead of what was typed since; a toast says why.
  reset();
  D("s:S", "first", [PIC]);
  answer = async () => { D("s:S", "typed since"); throw "gone"; };
  eq(await ctx.boxSend({ sid: "S" }, "s:S", toS("S")), { error: "gone" });
  eq([ctx.drafts["s:S"].text, ctx.drafts["s:S"].images.length, toasts], ["first\n\ntyped since", 1, ["Couldn't send: gone"]]);
  // failed() that deals with it: the box isn't written to, no toast.
  reset();
  D("s:S", "note");
  answer = async () => { throw "nope"; };
  let red = null;
  await ctx.boxSend({}, "s:S", { call: (t) => ctx.invoke("desk_item_act", { text: t }), pending: (t) => ({ t }), failed: (e, p) => { red = [e, p]; return true; } });
  eq([red, ctx.drafts["s:S"], toasts.length], [["nope", { t: "note" }], undefined, 0], "red on its Thread, the box left empty");
  // A quick phrase (compose.js sendPhrase): what you'd typed is put back in the box as soon as the send is under way;
  // one that didn't go leaves the box as you typed it, and the toast names it.
  reset(); answer = async () => { throw "x"; };
  D("s:S", "Go ahead"); ctx.phraseSending = true;
  const p2 = ctx.boxSend({ sid: "S" }, "s:S", toS("S")); ctx.phraseSending = false; D("s:S", "mine");
  await p2;
  eq([calls.length, ctx.drafts["s:S"].text, toasts], [1, "mine", ["Couldn't send “Go ahead”: x"]], "a phrase that didn't go");
  // clear "sent" (deskSendBack): emptied and saved only once it went.
  reset();
  D("back:7", "redo");
  let during = null;
  await ctx.boxSend({}, "back:7", { clear: "sent", call: async () => { during = ctx.drafts["back:7"]?.text; return {}; } });
  eq([during, ctx.drafts["back:7"], saves], ["redo", undefined, 1]);
  // save (the Desk's lead): saved once it went.
  reset(); D("desk:x", "hi");
  await ctx.boxSend({}, "desk:x", { save: true, call: async () => "" });
  assert.strictEqual(saves, 1);
  // Words given (a retry): the box is never touched; empty sends nothing; before() can stop it or change what's typed.
  reset(); D("s:S", "in the box");
  await ctx.boxSend({ sid: "S" }, "retry:7:1", { text: "again", images: [PIC], call: (t, im) => ctx.invoke("x", { t, n: im.length }) });
  eq([calls[0][1], ctx.drafts["s:S"].text, ctx.drafts["retry:7:1"]], [{ t: "again", n: 1 }, "in the box", undefined]);
  reset(); D("s:S", "  ");
  eq(await ctx.boxSend({ sid: "S" }, "s:S", toS("S")), { empty: true });
  D("s:S", "! ls");
  eq(await ctx.boxSend({ sid: "S" }, "s:S", { before: () => false, call: async () => "" }), { stopped: true });
  D("s:S", " /x");
  await ctx.boxSend({ sid: "S" }, "s:S", { before: (t, dr) => ` ${t}`, call: (t) => ctx.invoke("typed", { t }) });
  eq(calls.at(-1), ["typed", { t: " /x" }], "before() gives what's typed");
  // A call that throws at once is a failure like any other.
  reset(); D("s:S", "boom");
  eq(await ctx.boxSend({ sid: "S" }, "s:S", { call: () => { throw new Error("bad"); } }), { error: "Error: bad" });
  assert.strictEqual(ctx.drafts["s:S"].text, "boom");
  // The held message's pictures read back (pullHeld, deskHeldEdit).
  reset(); answer = async (cmd, a) => (a.path.endsWith("bad") ? null : "data:image/jpeg;base64,ZZ");
  eq(await ctx.boxHeldImages(["/x/a.jpg", "/x/bad"]), [{ name: "a.jpg", mime: "image/jpeg", data: "data:image/jpeg;base64,ZZ" }]);

  // ---- the chat box on it (chat/send.js deliver, chat/compose.js sendTo, pullHeld, sendAtEdit) ----
  const did = [];
  Object.assign(ctx, {
    isParked: () => false, setParked() {}, isHostedClaude: () => true, cmdOf: (k) => (ctx.drafts[k]?.text.trim() === "/nope" ? null : "x"),
    openBtw: (sid, q) => did.push(["btw", q]), openMcp: (sid) => did.push(["mcp"]), runLine: (sid, l) => did.push(["run", l]),
    drawAtChatEnd: () => did.push(["end", ctx.outbox.length]), sent: (sid, t, n, via) => did.push(["sent", t, n, via]),
    deskSaid: (t) => t, focusComposer: (k) => did.push(["focus", k]), lastSent: null,
  });
  ctx.sessions.S = { session_id: "S", state: "working", harness: "claude" };
  // Sent: shown in the chat while it goes, at the chat's end; "via" once it went.
  reset(); did.length = 0;
  D("s:S", "hello", [PIC]);
  eq(await ctx.sendTo("S", true), { ok: true, result: "sent via tmux" });
  eq([calls[0], ctx.outbox.at(-1).via, did, ctx.drafts["s:S"]], [["send_to_session", { sessionId: "S", text: "hello", images: [{ name: PIC.name, mime: PIC.mime, data: PIC.data }], now: true }], "tmux", [["end", 1], ["sent", "hello", 1, "sent via tmux"]], undefined], "sendTo");
  // Didn't go: out of the chat, back in the box with its pictures, ahead of what's typed since.
  reset(); ctx.outbox.length = 0;
  D("s:S", "again", [PIC]);
  answer = async () => { D("s:S", "since"); throw "down"; };
  eq(await ctx.sendTo("S"), { error: "down" });
  eq([ctx.outbox.length, ctx.drafts["s:S"].text, ctx.drafts["s:S"].images.length, toasts], [0, "again\n\nsince", 1, ["Couldn't send: down"]], "sendTo that didn't go");
  // The guards come first.
  reset(); ctx.state.restarting.S = true; D("s:S", "x");
  eq(await ctx.sendTo("S"), { held: "It's restarting. Send once it's back." });
  delete ctx.state.restarting.S;
  // "/btw", "/mcp", "!" lines and a command this session doesn't have never go; " /x" keeps its space.
  reset(); did.length = 0;
  for (const t of ["/btw why", "/mcp", "! ls"]) { D("s:S", t); eq(await ctx.sendTo("S"), { stopped: true }, t); assert.strictEqual(ctx.drafts["s:S"], undefined, t); }
  D("s:S", "/nope");
  eq(await ctx.sendTo("S"), { stopped: true });
  eq([did, calls.length, ctx.drafts["s:S"].text, toasts.length], [[["btw", "why"], ["mcp"], ["run", "! ls"]], 0, "/nope", 1], "dealt with here");
  D("s:S", " /x");
  await ctx.sendTo("S");
  eq(calls.at(-1)[1].text, " /x", "a leading space: a message");
  // pullHeld: the kept message back in the box, its pictures after the box's own, the cursor in it.
  reset(); did.length = 0;
  D("s:S", "mine", [PIC]);
  answer = async (cmd) => (cmd === "unhold_message" ? { text: "kept", images: ["/x/b.png"] } : "data:image/png;base64,BB");
  await ctx.pullHeld("S");
  eq([ctx.drafts["s:S"].text, ctx.drafts["s:S"].images.map((i) => i.name), did, saves > 0], ["kept\n\nmine", ["a.png", "b.png"], [["focus", "s:S"]], true], "pullHeld");
  // sendAtEdit: the message for later back in the box.
  reset(); answer = async () => "later"; D("s:S", "");
  await ctx.sendAtEdit("S");
  eq([ctx.drafts["s:S"].text, saves, draws], ["later", 1, 1], "sendAtEdit");

  // ---- the Desk's boxes on it (desk/act.js deskSendBox, deskSendItem, deskSendLead, deskSendBack, the retries) ----
  const pends = [];
  Object.assign(ctx, {
    deskPending: new Map(), deskSent: new Map(), deskAnswers: new Map(), deskQuestion: () => "Which?", deskOf: () => ({}),
    deskTab: "items", deskChatEnd: false, deskBackFor: null, deskAwayNoteSent: () => {},   // desk/away.js, not loaded here
    deskPend: (id, text, why, pics, more) => { const p = { id, text, why, pics, at: 1000, ...more }; pends.push(p); ctx.deskPending.set(id, [...(ctx.deskPending.get(id) || []), p]); return p; },
  });
  ctx.sessions.H = { session_id: "H", state: "working" };
  ctx.items[20] = { id: 20, title: "Tidy", state: "doing", holder: { kind: "session", sid: "H", name: "cue-20" } };
  const itemKey = 'desk:["P",20]';
  ctx.deskBoxes.set(itemKey, { project: "P", item: 20 });
  // A note to the item's holder: the box empties at once, the note's on its Thread, the hub gets it; saved once it has it.
  reset(); pends.length = 0;
  D(itemKey, " fix it ", [PIC]);
  let during2 = null;
  answer = async (cmd) => { during2 = ctx.drafts[itemKey]; return {}; };
  await ctx.deskSendBox(itemKey);
  eq([during2, calls, pends.map((p) => [p.why, p.text, p.act]), saves], [undefined, [["desk_item_act", { id: 20, act: "note", text: "fix it", images: [{ name: PIC.name, mime: PIC.mime, data: PIC.data }] }]], [["note", "fix it", "note"]], 1], "a note to the holder");
  // Didn't go: red on its Thread (not back in the box), the answer forgotten.
  reset(); pends.length = 0;
  ctx.items[20].state = "needs_you";
  D(itemKey, "yes");
  answer = async () => { throw "hub down"; };
  await ctx.deskSendBox(itemKey);
  eq([pends[0].failed, ctx.drafts[itemKey], ctx.deskAnswers.has(20), toasts], ["hub down", undefined, false, []], "a note that didn't go");
  // The guards look at its holder: nothing goes, the box is kept, no answer noted.
  reset(); pends.length = 0;
  ctx.state.restarting.H = true; D(itemKey, "now?");
  await ctx.deskSendBox(itemKey);
  eq([calls.length, pends.length, ctx.drafts[itemKey].text, toasts, ctx.deskAnswers.has(20)], [0, 0, "now?", ["It's restarting. Send once it's back."], false], "the holder restarting");
  delete ctx.state.restarting.H;
  // Held by no one: to the lead, named "About #…"; the box empties, saved once it went.
  reset(); pends.length = 0;
  ctx.items[21] = { id: 21, title: "New", state: "new" };
  const k21 = 'desk:["P",21]'; ctx.deskBoxes.set(k21, { project: "P", item: 21 });
  D(k21, "do it");
  answer = async () => "";
  await ctx.deskSendBox(k21);
  const mine = ctx.deskSent.get("P").at(-1);
  eq([calls[0], mine.text, mine.sent, pends.map((p) => [p.why, p.text]), saves, toasts, calls[1]], [["desk_send", { project: "P", text: "About #21 (New): do it", images: [] }], "About #21 (New): do it", 1000, [["lead", "do it"]], 1, ["Sent to lead-1"], ["desk_item_act", { id: 21, act: "wrote_lead", text: "do it" }]], "to the lead about an item");
  // To the lead, failed: red in its chat and on the item's Thread; then Retry sends it as it was.
  reset(); pends.length = 0;
  D(k21, "again"); answer = async () => { throw "no lead"; };
  await ctx.deskSendBox(k21);
  const m2 = ctx.deskSent.get("P").at(-1);
  eq([m2.failed, pends[0].failed, ctx.drafts[k21], toasts], ["no lead", "no lead", undefined, []], "to the lead, didn't go");
  reset(); answer = async () => "";
  await ctx.deskLeadRetry(m2);
  eq([calls[0], m2.failed, m2.sent, pends[0].failed], [["desk_send", { project: "P", text: "About #21 (New): again", images: [] }], null, 1000, null], "Retry to the lead");
  // Ready to check: sent back with your words; the box emptied only once it went.
  reset(); pends.length = 0;
  ctx.items[20].state = "to_check";
  D(itemKey, "not right");
  let during3 = null;
  answer = async () => { during3 = ctx.drafts[itemKey]?.text; return { holder: { name: "cue-20" } }; };
  await ctx.deskSendBox(itemKey);
  eq([calls[0], during3, ctx.drafts[itemKey], pends.map((p) => p.why), toasts], [["desk_item_act", { id: 20, act: "send_back", text: "not right", images: [] }], "not right", undefined, ["back"], ["Sent back to cue-20"]], "Send back from the item's box");
  // Send back's own box, didn't go: red on its Thread, the box closed.
  reset(); pends.length = 0;
  ctx.deskBackFor = 20; D("back:20", "redo"); answer = async () => { throw "busy"; };
  eq(await ctx.deskSendBack(20, "back:20"), false);
  eq([pends[0].failed, ctx.drafts["back:20"], ctx.deskBackFor], ["busy", undefined, null], "Send back that didn't go");
  // A picture alone that didn't go: it stays in its box, and a toast says why.
  reset(); pends.length = 0;
  D("back:20", "", [PIC]); answer = async () => { throw "busy"; };
  eq(await ctx.deskSendBack(20, "back:20"), false);
  eq([pends.length, ctx.drafts["back:20"].images.length, toasts], [0, 1, ["Not sent: busy"]], "a picture alone");
  // Retry on a message to the holder: the guards first (it stays red), then through its own box, "About …" before it.
  reset();
  const hp = { why: "holder", sid: "H", text: "hi", about: "About #20 (Tidy): ", at: 5, failed: "x", key: itemKey };
  ctx.deskPending.set(20, [hp]);
  ctx.sessions.H.state = "deciding";
  await ctx.deskMsgRetry(20, 5);
  eq([hp.failed, calls.length, toasts], ["x", 0, ["It's asking you something first. Answer that, then send."]], "a holder retry held");
  ctx.sessions.H.state = "working";
  D(itemKey, "mine");
  await ctx.deskMsgRetry(20, 5);
  eq([hp.failed, calls[0], ctx.drafts[itemKey].text, ctx.drafts["retry:20:5"]], [null, ["send_to_session", { sessionId: "H", text: "About #20 (Tidy): hi", images: [], now: false }], "mine", undefined], "a holder retry");

  // ---- btw and a forward (chat/send.js askBtw, sheets.js sendForward, on boxSend since step 3d) ----
  // btw: asked at once in its panel (the box emptied), then its answer; no guards, the session is never written to.
  reset();
  ctx.btwLog.clear(); ctx.sessions.A = { session_id: "A", state: "deciding" };
  D("btw:A", "  why?  ");
  let shown = null;
  answer = async () => { shown = [ctx.btwLog.get("A")[0].a, ctx.drafts["btw:A"], draws]; return "because"; };
  await ctx.askBtw("A", ctx.drafts["btw:A"].text);
  eq([calls, shown, ctx.btwLog.get("A"), draws, toasts], [[["btw", { sessionId: "A", question: "why?" }]], [null, undefined, 1], [{ q: "why?", a: "because", err: null }], 2, []], "btw asked and answered");
  // Didn't go: why, in its place; nothing put back, no toast.
  reset(); answer = async () => { throw "no copy"; };
  D("btw:A", "typed");
  await ctx.askBtw("A", "from /btw");
  eq([ctx.btwLog.get("A")[1], ctx.drafts["btw:A"], draws, toasts], [{ q: "from /btw", a: null, err: "no copy" }, undefined, 2, []], "btw failed; a /btw empties its box too");
  reset(); D("btw:A", "kept");
  await ctx.askBtw("A", "   ");
  eq([calls.length, ctx.drafts["btw:A"].text, ctx.btwLog.get("A").length], [0, "kept", 2], "nothing to ask");
  delete ctx.sessions.A;
  // Forward: the note, then the message, the menu shut; a toast either way, nothing put back.
  const fwdTo = { session_id: "F", project: "proj", state: "idle" };
  const sess3 = ctx.state.sessions;
  ctx.state.sessions = [fwdTo]; ctx.sessionOf = (sid) => ctx.state.sessions.find((s) => s.session_id === sid) || null;
  reset(); ctx.sheet = "forward"; ctx.forward = { to: "F", from: "cue-1", text: "the reply" };
  D("fwd", " see this ");
  let duringFwd = null;
  answer = async () => { duringFwd = [ctx.sheet, ctx.forward, ctx.drafts.fwd, draws]; return "sent"; };
  await ctx.sendForward();
  eq([calls, duringFwd, toasts, saves], [[["send_to_session", { sessionId: "F", text: "see this\n\n[Forwarded from the cue-1 session]\nthe reply", images: [] }]], [null, null, undefined, 1], ["Sent to proj"], 0], "forwarded with a note");
  reset(); ctx.sheet = "forward"; ctx.forward = { to: "F", from: "cue-1", text: "x" }; answer = async () => { throw "gone"; };
  await ctx.sendForward();
  eq([calls[0][1].text, toasts, ctx.drafts.fwd, draws], ["[Forwarded from the cue-1 session]\nx", ["Couldn't send to proj: gone"], undefined, 1], "forward without a note, didn't go");
  reset(); fwdTo.state = "deciding"; ctx.forward = { to: "F", from: "cue-1", text: "x" }; D("fwd", "n");
  await ctx.sendForward();
  eq([calls.length, toasts, ctx.drafts.fwd.text, !!ctx.forward], [0, ["proj is asking you something first. Answer that, then send."], "n", true], "forward held");
  ctx.state.sessions = sess3; ctx.sessionOf = (sid) => ctx.sessions[sid] || null; ctx.forward = null; ctx.sheet = null;

  console.log("the message box: markup as today's boxes draw it, put back, the chat's end, send");
})().catch((e) => { console.error(e); process.exit(1); });
