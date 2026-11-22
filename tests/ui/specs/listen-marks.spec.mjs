// Listen's marks on the reply it reads (chat/listen.js): only the sentence being read is lit after a click elsewhere,
// the pointer stays the plain arrow with nothing selected, a list's points, a table's rows and a code block are marked
// as they're read, and the marks move without being redrawn whole (#672).
import { test, expect } from "./cue.mjs";

const REPLY = "First sentence of the reply is here. Second sentence follows it right away. Third one ends the paragraph.\n\n- A list point about apples.\n- Another point about pears.\n\n1. Step one of the plan.\n\n| File | Change |\n| --- | --- |\n| docs | new words |\n\n```\ncargo test\n```\n\nIt is in `src-tauri/src/ipc/docs.rs` now.";

/** The reply open in its chat and Listen started on it, each sentence held until `__end()`; `plain`: Skip code & tables. */
async function listen(cue, page, { plain = false } = {}) {
  await cue.open("board-waiting");
  await page.evaluate((on) => { localStorage.setItem("cue.listenPlain", on ? "1" : "0"); listenPlain = on; }, plain);
  await cue.change((s, t) => { s.sessions.find((x) => x.session_id === "relay").thread.at(-1).text = t; }, REPLY);
  await page.locator('.nrow[data-big="n1"]').click();
  await page.evaluate(() => {
    window.__spoken = [];
    // (On the prototype: WebKit doesn't take these set on the object itself.)
    SpeechSynthesis.prototype.speak = (u) => { window.__spoken.push(u.text); window.__u = u; };
    SpeechSynthesis.prototype.cancel = () => {};
    window.__end = () => window.__u.onend();
    window.__word = (at, len) => window.__u.onboundary({ name: "word", charIndex: at, charLength: len });
    // Every change to the marks from here on: the registry's own (set, delete) and each Highlight's (add, delete, clear).
    window.__ops = [];
    const name = (h) => [...CSS.highlights].find(([, x]) => x === h)?.[0] || "?";
    for (const m of ["set", "delete", "clear"]) { const was = CSS.highlights[m].bind(CSS.highlights); CSS.highlights[m] = (...a) => { window.__ops.push(`registry.${m} ${a[0] ?? ""}`.trim()); return was(...a); }; }
    for (const m of ["add", "delete", "clear"]) { const was = Highlight.prototype[m]; Highlight.prototype[m] = function (...a) { window.__ops.push(`${name(this)}.${m}`); return was.apply(this, a); }; }
  });
  const reply = page.locator(".active-pane .cv-agent", { hasText: "First sentence of the reply" });
  await reply.locator("[data-listen]").click();
  await expect.poll(() => page.evaluate(() => window.__spoken.length)).toBe(1);
  return reply;
}
const marks = (page) => page.evaluate(() => Object.fromEntries(["cue-said", "cue-say", "cue-word"].map((n) => [n, [...(CSS.highlights.get(n) || [])].map((r) => r.toString())])));
/** The sentence after this one, `n` times. */
async function on(page, n = 1) {
  for (let k = 0; k < n; k++) { const was = await page.evaluate(() => window.__spoken.length); await page.evaluate(() => window.__end()); await expect.poll(() => page.evaluate(() => window.__spoken.length)).toBe(was + 1); }
}
/** A click on `text` in the reply, at its start; the chat scrolled to it first, as you would (the wheel), when the
 *  reading has taken it out of view. */
async function clickOn(page, reply, text) {
  const where = () => reply.locator(".msg").evaluate((msg, t) => {
    const w = document.createTreeWalker(msg, NodeFilter.SHOW_TEXT), box = msg.closest("[data-chat]").getBoundingClientRect();
    for (let n; (n = w.nextNode());) { const i = n.data.indexOf(t); if (i >= 0) { const r = document.createRange(); r.setStart(n, i); r.setEnd(n, i + t.length); const b = r.getClientRects()[0]; return { x: b.left + 4, y: b.top + b.height / 2, off: b.top < box.top + 30 ? b.top - box.top - 80 : b.bottom > box.bottom - 60 ? b.bottom - box.bottom + 80 : 0, mid: [box.left + box.width / 2, box.top + box.height / 2] }; } }
    return null;
  }, text);
  let at = await where();
  if (at.off) {
    await page.mouse.move(...at.mid);
    await page.mouse.wheel(0, at.off);
    await expect.poll(async () => (at = await where()).off).toBe(0);
  }
  await page.mouse.click(at.x, at.y);
}

test("a click elsewhere in the reply reads from there, and only that sentence is lit: the old marks are taken off, never left", async ({ cue, page }) => {
  const reply = await listen(cue, page);
  await on(page, 4);   // down to the second list point
  await page.evaluate(() => window.__word(8, 5));
  expect(await marks(page)).toMatchObject({ "cue-say": ["Another point about pears."], "cue-word": ["point"] });
  expect((await marks(page))["cue-said"]).toHaveLength(4);
  const before = await page.evaluateHandle(() => ["cue-said", "cue-say", "cue-word"].map((n) => CSS.highlights.get(n)));
  // Back to the reply's second sentence: nothing after it stays gray or lit, and no word is marked.
  await clickOn(page, reply, "Second sentence");
  await expect.poll(() => page.evaluate(() => window.__spoken.at(-1))).toBe("Second sentence follows it right away.");
  expect(await marks(page)).toEqual({ "cue-said": ["First sentence of the reply is here."], "cue-say": ["Second sentence follows it right away."], "cue-word": [] });
  // The cause: a mark swapped for a new one (or deleted) in the registry isn't repainted by the Mac's WebKit, so the
  // old one stayed on screen. The marks are the same three objects from start to end, changed range by range.
  expect(await page.evaluate((was) => ["cue-said", "cue-say", "cue-word"].every((n, i) => CSS.highlights.get(n) === was[i]), before)).toBe(true);
  // (Each registered once, at the first sentence; after that the registry is never written to.)
  expect(await page.evaluate(() => window.__ops.filter((o) => o.startsWith("registry")))).toEqual(["registry.set cue-said", "registry.set cue-say", "registry.set cue-word"]);
  // Stop: every mark off, by its ranges.
  await page.locator("[data-listen-stop]").first().click();
  expect(await marks(page)).toEqual({ "cue-said": [], "cue-say": [], "cue-word": [] });
});

for (const scheme of ["light", "dark"]) {
  test(`while a reply is read the pointer over it is the plain arrow and nothing in it gets selected; the marks are the design's colors (${scheme})`, async ({ cue, page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    const reply = await listen(cue, page);
    const p = reply.locator(".msg p").first();
    const look = () => p.evaluate((el) => { const c = getComputedStyle(el); return { cursor: c.cursor, select: c.userSelect || c.webkitUserSelect }; });
    expect(await look()).toEqual({ cursor: "default", select: "none" });
    // A double click (it would select a word) and a drag (a run of text): it reads from there, with nothing selected.
    const box = await p.boundingBox();
    await page.mouse.dblclick(box.x + 60, box.y + 8);
    await page.mouse.move(box.x + 10, box.y + 8); await page.mouse.down(); await page.mouse.move(box.x + 200, box.y + 8); await page.mouse.up();
    expect(await page.evaluate(() => String(getSelection()))).toBe("");
    expect((await marks(page))["cue-say"]).toEqual(["First sentence of the reply is here."]);
    // The marks: the selected card's lime under the sentence, the accent under the word, the read text muted.
    const tokens = await p.evaluate((el) => {
      const made = (v) => { const d = document.createElement("i"); d.style.color = `var(${v})`; el.append(d); const c = getComputedStyle(d).color; d.remove(); return c; };
      const hl = (n) => getComputedStyle(el, `::highlight(${n})`);
      return { say: [hl("cue-say").backgroundColor, made("--sel")], sayInk: [hl("cue-say").color, made("--on-sel")], word: [hl("cue-word").backgroundColor, made("--accent")], said: [hl("cue-said").color, made("--muted")] };
    });
    for (const [got, want] of Object.values(tokens)) expect(got).toBe(want);
    if (process.env.SHOT_DIR) await reply.screenshot({ path: `${process.env.SHOT_DIR}/listen-marks-${scheme}.png` });
    // Another reply, and this one once it stops: text to select again, under the text cursor.
    await page.locator("[data-listen-stop]").first().click();
    expect(await look()).toEqual({ cursor: "auto", select: "text" });
    await expect(page.locator(".msg.ls-on")).toHaveCount(0);
  });
}

test("a list's points, a table's rows and a code block are marked as they're read, word by word, and a click on a row reads from it", async ({ cue, page }) => {
  const reply = await listen(cue, page);
  const say = async () => (await marks(page))["cue-say"].join("").replace(/\s+/g, " ");
  await on(page, 3);
  expect(await say()).toBe("A list point about apples.");
  await on(page);
  expect(await say()).toBe("Another point about pears.");
  await on(page);
  expect(await say()).toBe("Step one of the plan.");
  // The table: its head row, then each row, said with a pause between cells and marked across them.
  await on(page);
  expect(await page.evaluate(() => window.__spoken.at(-1))).toBe("File, Change");
  expect(await say()).toBe("FileChange");
  await on(page);
  expect(await page.evaluate(() => window.__spoken.at(-1))).toBe("docs, new words");
  expect(await say()).toBe("docsnew words");
  // The word it's saying, in its cell: "new" (the 7th character of what's said).
  await page.evaluate(() => window.__word(6, 3));
  expect((await marks(page))["cue-word"]).toEqual(["new"]);
  // The row is lit where it's drawn: the lime is on its cells' text.
  const lit = await reply.locator("tbody td").first().evaluate((td) => [...CSS.highlights.get("cue-say")].some((r) => r.intersectsNode(td.firstChild)));
  expect(lit).toBe(true);
  // The code block, whole.
  await on(page);
  expect(await page.evaluate(() => window.__spoken.at(-1))).toBe("Code block.");
  expect(await say()).toBe("cargo test");
  // Read rows go gray with the rest; a click on the head row reads from it again.
  await on(page);
  expect((await marks(page))["cue-said"]).toContain("docsnew words");
  await clickOn(page, reply, "Change");
  await expect.poll(() => page.evaluate(() => window.__spoken.at(-1))).toBe("File, Change");
  expect(await say()).toBe("FileChange");
});

test("the marks don't flicker: as the word moves only the word's mark changes, and a redraw of the chat puts them back as they were", async ({ cue, page }) => {
  await listen(cue, page, { plain: true });
  await on(page, 2);
  expect((await marks(page))["cue-say"]).toEqual(["Third one ends the paragraph."]);
  await page.evaluate(() => { window.__ops = []; });
  await page.evaluate(() => { window.__word(0, 5); window.__word(6, 3); window.__word(6, 3); window.__word(10, 4); });
  expect((await marks(page))["cue-word"]).toEqual(["ends"]);
  // Three words: the word's mark moved three times (the old range off, the new one on; the same word again: nothing).
  // The sentence's mark and the read text's were not touched, and nothing was swapped in the registry.
  expect(await page.evaluate(() => window.__ops)).toEqual(["cue-word.add", "cue-word.delete", "cue-word.add", "cue-word.delete", "cue-word.add"]);
  // The chat drawn again (the state comes in again; a draw lifts the chat out and back, which empties every range in
  // it): the marks are back on the same text before anything is painted, still the same three, nothing swapped.
  const before = await marks(page);
  await page.evaluate(() => { window.__ops = []; });
  await cue.change(() => {});
  await page.evaluate(() => renderMain());
  expect(await page.evaluate(() => window.__ops.filter((o) => o.startsWith("registry")))).toEqual([]);
  expect(await marks(page)).toEqual(before);
  expect(before).toMatchObject({ "cue-said": ["First sentence of the reply is here.", "Second sentence follows it right away."], "cue-say": ["Third one ends the paragraph."], "cue-word": ["ends"] });
  // With Skip code & tables a path is said by its last part ("It is in docs now."): the word marked is the one heard,
  // not one a few letters into the path.
  await on(page, 4);   // the two list points, the step, then the last line (the table and the code are left out)
  expect(await page.evaluate(() => window.__spoken.slice(3))).toEqual(["A list point about apples.", "Another point about pears.", "Step one of the plan.", "It is in docs now."]);
  await page.evaluate(() => window.__word(14, 3));
  expect((await marks(page))["cue-word"]).toEqual(["now."]);
  await page.evaluate(() => window.__word(9, 4));   // "docs": said for the path, not a word in the text
  expect((await marks(page))["cue-word"]).toEqual([]);
});
