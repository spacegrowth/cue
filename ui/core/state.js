// Cue UI core — the Tauri bridge (its invoke, kept in order: invoke.js), the window's state, and the helpers every view shares (formatting, markdown, names, caches).
// (ui/core/: the window's state here; addon.js the add-on's slots; helpers.js names, sessions, bars; md.js markdown; colors.js projects and their colors.)
const T = window.__TAURI__;

let state = { items: [], history: [], sessions: [], settings: {}, connections: {}, window_ms: 1800000, now_ms: Date.now() };
let clockSkew = 0;          // backend clock − browser clock, so bars line up with the backend's timestamps
let sheet = null;           // null | "settings"
let view = "board";         // the switch in the header: "board" | "read" | "insights" (or one of addOn.views); "terminal" from the terminal button; "history" from All history →
let histOpen = null;        // the History row that's open (one at a time)
let histShown = null;       // the open row we've already scrolled to (so a redraw doesn't yank it back)
let active = null;          // { id, sid }: what the Active pane shows. Sticky: it stays after you act.
let holdChat = null;        // earlier messages just went in above: keep this distance from the chat's bottom
let activeKey = null;       // last rendered Active target + message count, to scroll the chat on change
let menuFor = null;         // decision card whose "Always…" menu is open
let redirectFor = null;     // decision card showing its "tell it instead" field
const cleared = new Set();  // finished turns you cleared here (their fading row says so)
const ghosts = new Map();   // id -> { it, at, h }: just-answered rows that linger in Waiting (h: the card's height before)
let folding = false;        // a ✓ row is folding away: redraws wait, or they'd pop it back mid-fold
const answering = new Map(); // id -> when you answered it here: it fades at once, and folds LINGER_MS after that
const unresolvedCard = (it) => it?.status === "pending" && !answering.has(it.id);
/** A live decision can be answered now. Finished-turn cards are evaluated separately. */
const actionableRequest = (it) => unresolvedCard(it) && it.kind !== "waiting";
const openMsgs = new Set(); // older agent messages unfolded in the Active chat
const openAsks = new Set(); // messages whose added note (voice line, status ask) is open, word for word
const openFollow = new Set();  // cards whose "After a stop hook" is expanded
let dictating = null;       // { key, base, stopping } — the text box you're dictating into, and what it held before
let renaming = null;        // the session whose name is being edited in the Active pane
let revealed = "";             // the selection last scrolled into view (only a new one scrolls)
const seenCards = new Set();  // Waiting cards already on screen once (only new arrivals animate)
let cardsPrimed = false;        // the first draw just takes stock: nothing "arrives" at launch
let slideDir = 0;           // +1 / -1: the next redraw slides the Active pane in from the right / left
let chipNav = null;         // the header chip you're stepping through ("waiting" | "asking")
let forward = null;         // { key, text, from, fromSid, to } — an agent message being sent on to another session
let lightbox = null;        // { srcs: [...], i } — an image opened full size
let lastSent = null;        // { sid, text, images, via, at }: how the last message went out
const drafts = {};          // per item: { text, choices: { [question]: label | Set } }
