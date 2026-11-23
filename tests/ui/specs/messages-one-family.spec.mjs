// One message family (#501, docs/cue-messages.md): every kind of message, wherever it shows, is drawn with the shared
// classes of ui/styles/messages.css. Whatever is said is one body size, everything around it one smaller meta size,
// the same in light and dark; no edge stripe, no hover text; messages.css has no colour of its own outside its tokens.
import { readFileSync } from "node:fs";
import { test, expect, openDesk } from "./cue.mjs";

const BODY = "15px", META = "13px";   // a chat's words at Slack's size (#703)

/** `read` run on what `loc` finds now (`all`: every match, else the first), again when a redraw replaced it between
 *  finding it and reading it: a chat redraws as its older messages and steps come in, and a replaced element reads as
 *  no size and no colour (#609). */
async function onPage(loc, read, all) {
  for (let n = 0; ; n++) {
    const got = all
      ? await loc.evaluateAll((es, f) => (es.every((e) => e.isConnected) ? (0, eval)(`(${f})`)(es) : null), read.toString())
      : await loc.first().evaluate((e, f) => (e.isConnected ? (0, eval)(`(${f})`)(e) : null), read.toString());
    if (got !== null || n === 20) return got;
  }
}
/** The font size of each element `loc` matches. */
const sizes = (loc) => onPage(loc, (es) => es.map((e) => getComputedStyle(e).fontSize), true);
/** `loc` is drawn as the family's card: head at the meta size, its words at the body size, no edge stripe. */
async function isCard(loc) {
  await expect(loc.first()).toBeVisible();
  for (const s of await sizes(loc.locator(":scope > .mg-h"))) expect(s).toBe(META);
  for (const s of await sizes(loc.locator(".mg-t"))) expect(s).toBe(BODY);
  for (const s of await sizes(loc.locator(":scope > .mg-f"))) expect(s).toBe(META);
  // No edge stripe: the left border is no wider, nor another colour, than the right one.
  for (const [lw, rw, lc, rc] of await onPage(loc, (es) => es.map((e) => { const c = getComputedStyle(e); return [c.borderLeftWidth, c.borderRightWidth, c.borderLeftColor, c.borderRightColor]; }), true)) {
    expect(lw).toBe(rw);
    expect(lc).toBe(rc);
  }
}
/** Its words read on what's behind them, in this scheme: their colour far from the nearest background's. */
async function readsOn(loc) {
  const [fg, bg] = await onPage(loc, (e) => {
    // Any CSS colour (rgb, or the oklch a color-mix computes to) as sRGB, by painting it.
    const cx = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
    const lum = (c) => { cx.clearRect(0, 0, 1, 1); cx.fillStyle = c; cx.fillRect(0, 0, 1, 1); const [r, g, b, a] = cx.getImageData(0, 0, 1, 1).data; return [0.2126 * r + 0.7152 * g + 0.0722 * b, a / 255]; };
    let n = e, b = [255, 0];
    while (n && n.nodeType === 1) { b = lum(getComputedStyle(n).backgroundColor); if (b[1] > 0.5) break; n = n.parentElement; }
    if (b[1] <= 0.5) b = lum(getComputedStyle(document.body).backgroundColor);
    return [lum(getComputedStyle(e).color)[0], b[0]];
  }, false);
  expect(Math.abs(fg - bg)).toBeGreaterThan(90);
}
const noHover = (page) => expect(page.locator("[title]")).toHaveCount(0);

/** Entries added to session `sid`'s thread, as the hub notes them, a millisecond apart. */
const push = (page, sid, entries) => page.evaluate(([sid, es]) => {
  const th = sessionOf(sid).thread, t = Date.now() - 60_000;
  es.forEach((e, n) => th.push({ at_ms: t + n, ...e }));
  renderMain();
}, [sid, entries]);

for (const scheme of ["light", "dark"]) {
  test.describe(`one message family (${scheme})`, () => {
    test.beforeEach(async ({ page }) => { await page.emulateMedia({ colorScheme: scheme }); });

    test("a session's chat: Cue's card, your bubble, the agent's reply, an ask and the status line", async ({ page, cue }) => {
      await page.setViewportSize({ width: 1500, height: 950 });
      await cue.open("desk");
      await openDesk(page);
      await push(page, "host", [
        { role: "you", text: "Desk item #d2 is resumed: carry on from where you stopped. Say DONE: #d2 on a line of its own when it's done." },
        { role: "you", text: "Can you make the header blue?" },
        { role: "agent", text: "Bash: cargo test" },
        { role: "you", text: "allowed" },
        { role: "agent", text: "Made it blue." },
      ]);
      await page.evaluate(() => deskShow({ kind: "chat", sid: "host" }));
      // Opening the chat loads its older messages and its steps, each redrawing it once it's in (chat/thread.js): measure
      // after those, or a card found before a redraw is gone by the time it's read (#609).
      await page.waitForFunction(() => stepFeeds.has("host") && !stepsBusy && older.get("host") && !older.get("host").busy);
      const chat = page.locator('.dk-right [data-desk-chat="host"]');
      const you = chat.locator(".mg-you", { hasText: "make the header blue" });
      await expect(you.locator(".mg-text")).toHaveCSS("font-size", BODY);
      await expect(chat.locator(".mg-agent").last().locator(".mg-text")).toHaveCSS("font-size", BODY);
      // Slack's spacing too (#703): the words on a 22px line, in your bubble and the reply alike.
      await expect(you.locator(".mg-text")).toHaveCSS("line-height", "22px");
      await expect(chat.locator(".mg-agent").last().locator(".mg-text")).toHaveCSS("line-height", "22px");
      await expect(chat.locator(".mg-agent").last().locator(".mg-h")).toHaveCSS("font-size", META);
      await isCard(chat.locator(".mg.ask"));
      await expect(chat.locator(".mg.ask pre.mg-code")).toHaveCount(1);
      // Cue's card: a session that isn't a lead shows its chat as it is (#744), so the card is in it, drawn as the family's.
      await isCard(chat.locator(".mg.cue-msg"));
      await readsOn(chat.locator(".mg.cue-msg .mg-t"));
      await noHover(page);
    });

    test("an item's page: its holder's chat about it draws Cue's note as its own chat does", async ({ page, cue }) => {
      await cue.open("desk");
      await openDesk(page);
      const sid = await page.evaluate(() => state.desk.projects.flatMap((d) => d.items).find((i) => i.id === "d2")?.holder?.sid);
      await push(page, sid, [{ role: "you", text: "Note from the Desk lead on Desk item #d2 (Phrases, finishing): Check the dark theme too." }]);
      await page.evaluate(() => deskShow({ kind: "item", id: "d2" }));
      // Cue's card is the Desk's own (#688): a count line in the item's chat, the card in Activity.
      await expect(page.locator(".dk-right .dkn-sp[data-desk-page='session'] .dk-cnt")).toHaveCount(1);
      await expect(page.locator(".dk-right .mg.cue-msg")).toHaveCount(0);
      await expect(page.locator(".dk-right .dk-cue")).toHaveCount(0);
      // The holder is the lead: its chat draws Cue's notes as short lines (#616), so Activity shows that line, not a card.
      await page.evaluate(() => deskOpenActivity());
      const line = page.locator(".dk-moves .dk-cue").filter({ hasText: "Check the dark theme too" });
      await expect(line).toHaveCount(1);
      await expect(page.locator(".dk-moves .mg.cue-msg").filter({ hasText: "Check the dark theme too" })).toHaveCount(0);
      expect(await sizes(line.locator(".mg-line"))).toEqual([META]);
      expect(await sizes(line.locator(".dk-cue-w"))).toEqual([BODY]);
      await readsOn(line.locator(".dk-cue-w"));
      await noHover(page);
    });

    test("the lead's chat: its reply, a NOTE, its item lines, Listen/Read, an ask and its answer, Ready to check, Compacted: one body size, one meta size", async ({ page, cue }) => {
      await page.setViewportSize({ width: 1500, height: 1400 });
      await cue.open("desk");
      await openDesk(page);
      await push(page, "lead", [
        { role: "agent", text: "Routed the header work.\n\nROUTE: #d2 -> host\nROUTE: #d3 -> host | after: #d2\nITEM: task | Dark theme pass" },
        { role: "agent", text: "NOTE: #d2 | Check the dark theme too." },
        { role: "agent", text: "Two ways to place them.\n\nASK: Phrases left or right of Send? | choices: Left / Right" },
        { role: "agent", text: "asked: Phrases left or right of Send?" },
        { role: "you", text: "answered: Left", decision: true },
      ]);
      await page.evaluate(() => { const l = sessionOf("lead"); l.compacted_ms = Date.now() - 30_000; l.state = "idle"; renderMain(); });
      const chat = page.locator(".dk-chat");
      // The item lines and the NOTE are the Desk's own (#688): a count line here, each in Activity (below).
      await expect(chat.locator(".dk-cnt")).toHaveCount(1);
      await expect(chat.locator('[data-mg="note"]')).toHaveCount(0);
      await expect(chat.locator('[data-mg="check"]')).toHaveCount(1);
      await expect(chat.locator(".mg.ask .mg-st")).toContainText("Left");
      await expect(chat.locator(".mg-status", { hasText: "Compacted" })).toHaveCount(1);
      await expect(chat.locator(".mg-btn", { hasText: /Listen|Read/ }).first()).toBeVisible();
      // Whatever is said: one size.
      const bodies = chat.locator(":is(.mg-agent .mg-text, .mg-t, .mg-line)");
      expect(new Set(await sizes(bodies))).toEqual(new Set([BODY]));
      // Everything around it: one smaller size (heads, "who · when", feet, Listen/Read, the status line).
      const meta = chat.locator(":is(.mg-h, .mg > .mg-f, .mg-status, .mg-btn)");
      expect(new Set(await sizes(meta))).toEqual(new Set([META]));
      await isCard(chat.locator('.mg[data-mg="check"]'));
      await page.evaluate(() => deskOpenActivity());
      const act = page.locator(".dk-moves");
      await expect(act.locator(".mg-line")).toContainText([/Gave/, /waits on/, /Added/]);
      await expect(act.locator('[data-mg="note"]')).toHaveCount(1);
      expect(new Set(await sizes(act.locator(":is(.mg-t, .mg-line)")))).toEqual(new Set([BODY]));
      expect(new Set(await sizes(act.locator(":is(.mg-h, .mg > .mg-f)")))).toEqual(new Set([META]));
      await isCard(act.locator('.mg[data-mg="note"]'));
      await readsOn(act.locator('[data-mg="note"] .mg-t'));
      await noHover(page);
    });

    test("another Desk's message to the lead is the band in that Desk's colour (#640 option B), not the family's card", async ({ page, cue }) => {
      await cue.open("desk");
      await openDesk(page);
      await push(page, "lead", [{ role: "peer", from: "site-lead", from_sid: "site", text: "Can you take the header colours from ours?" }]);
      const band = page.locator('.dk-chat .dk-band[data-band="from"]');
      await expect(band).toHaveCount(1);
      await expect(page.locator('.dk-chat .mg[data-mg="peer"]')).toHaveCount(0);
      await expect(band.locator(".dk-band-desk")).toHaveCount(1);
      await readsOn(band.locator(".dk-band-t"));
      await noHover(page);
    });

    test("the notch: a crew lead's chat draws What Cue sent as the family's card", async ({ page, cue }) => {
      await page.setViewportSize({ width: 560, height: 700 });
      await cue.openMini("world-team", "notch", { frozen: true });
      await page.evaluate(() => document.body.dispatchEvent(new MouseEvent("mouseenter")));
      const card = page.locator(".m-chat .mg").first();
      await isCard(card);
      await readsOn(card.locator(".mg-t"));
      await noHover(page);
    });
  });
}

test("messages.css has no colour of its own outside its tokens (:root and its dark blocks)", () => {
  let css = readFileSync(new URL("../../../ui/styles/messages.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  // Drop each token block: :root { … } (light, and the dark ones under @media or [data-theme]).
  css = css.replace(/:root[^{]*\{[^}]*\}/g, "");
  expect(css.match(/#[0-9a-f]{3,8}\b/gi) || []).toEqual([]);
});
