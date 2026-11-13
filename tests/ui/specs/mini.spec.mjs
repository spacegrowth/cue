// The mini window (ui/mini.html), its chat drawn by Cue's own chat code: a crew lead's chat (world-team) at the notch
// and on the rail, each a screenshot compared with its baseline (shots.mjs), as screens.spec.mjs does the main window's.
// mini-notch: the lead's chat under the notch's head, its crew's tray closed to its line: Cue's bubble (its member's
// review folded in), the lead's steps under it, the box with its quick phrases. mini-rail: the rail's panel, the tray
// open on hub's ask (Deny and Allow on its row). The fixture checks every script ran (no page or console error).
import { test, expect } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { compareShot } from "./shots.mjs";
import { pillProblems } from "./pills.mjs";

// A 14-inch laptop's screen: the chat is a share of its height (mini.js CAP), the window as wide as the chat (WIDTH,
// and the notch's shoulders either side).
const SCREEN = { width: 1512, height: 982 };
const WINDOW = { notch: { width: 560 + 2 * 14, height: Math.round(SCREEN.height * 0.5) }, rail: { width: 420, height: Math.round(SCREEN.height * 0.45) } };

/** The lead's last turn: what it did after Cue asked for docs's review (world-team's thread), before its own ask. */
function leadSteps({ sessionId }, now) {
  if (sessionId !== "lead") return { version: 0, turns: [] };
  const step = (id, kind, verb, subject, result, ms) => ({ t: "step", id, kind, verb, subject, mono: true, result, bad: false, done: true, at_ms: now + ms });
  return { version: 1, turns: [{ at_ms: now + 0.1, prompt: "", more: 0, items: [
    step("l1", "read", "Read", "docs/dev/cue-teams.md", "212 lines", 0.2),
    step("l2", "run", "Ran", "git diff --stat crew/docs", "1 file +18 −2", 0.3),
    step("l3", "agent", "Delegated", "Review docs's work", "done", 0.4),
  ] }] };
}

/** The mini window opened on the lead's chat: the pointer over it (the notch) or its tab pressed (the rail). */
async function openLead(cue, page, shape) {
  await page.setViewportSize(WINDOW[shape]);
  // The page's screen is its viewport here: the laptop's, as the window sizes its chat from it.
  await page.addInitScript((h) => Object.defineProperty(Screen.prototype, "height", { get: () => h }), SCREEN.height);
  await cue.openMini("world-team", shape, { frozen: true });
  await cue.override("session_steps", leadSteps, NOW);
  if (shape === "notch") await page.evaluate(() => document.body.dispatchEvent(new MouseEvent("mouseenter")));
  else await page.locator("[data-out]").click();
  await page.locator('.m-tab.on[data-mopen="lead"]').waitFor();
  await page.locator(".m-chat .ap-chat .stx-steps").waitFor();
}
/** The crew's tray open or closed, as `open` says (it opens by itself while one of the crew waits on you). */
async function showCrew(page, open) {
  const b = page.locator(".m-chat [data-mt-open]").first();
  if ((await b.getAttribute("aria-expanded")) !== String(open)) await b.click();
  await expect(b).toHaveAttribute("aria-expanded", String(open));
}

for (const shape of ["notch", "rail"]) {
  test(`mini: ${shape}, a crew lead's chat`, async ({ cue, page }) => {
    await openLead(cue, page, shape);
    // Cue's own bubbles (not yours), each with its member's review folded in, and no copy of the chat of its own.
    await expect(page.locator(".m-chat .cue-you")).toHaveCount(2);
    await expect(page.locator(".m-chat .cue-you [data-ask-open$=':review']")).toHaveCount(2);
    await expect(page.locator(".m-chat .crew-tray")).toHaveCount(1);
    await expect(page.locator(".m-chat .quick .qp")).toHaveCount(3);
    await showCrew(page, shape === "rail");
    if (shape === "rail") await page.locator('.m-chat .mt-row.need [data-act="allow"]').waitFor();
    else {
      // The notch's chat is short: scrolled up to Cue's last bubble, the lead's steps under it (once it's done going
      // to its end as it opened, mini.js toLatest).
      await page.waitForTimeout(300);
      await page.evaluate(() => { const c = document.querySelector("[data-chat]"), b = [...c.querySelectorAll(".cue-you")].at(-1); c.scrollTop += b.getBoundingClientRect().top - c.getBoundingClientRect().top - 8; });
    }
    await page.mouse.move(0, 0);
    await page.evaluate(() => { document.activeElement?.blur(); return document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))); });
    const name = `mini-${shape}`;
    const r = compareShot(name, await page.screenshot({ animations: "disabled", caret: "hide" }));
    expect(r.baseline, `baseline for ${name}`).not.toBe("missing");
    expect(await pillProblems(page, "body"), `${name}: small labels that aren't a full capsule`).toEqual([]);
    expect(r.differ, `${name}: pixels that differ from tests/ui/baseline/${name}.png (see tests/ui/shots/${name}.diff.png)`).toBe(0);
  });
}

// The chat's buttons are Cue's own (app/clicks.js CHAT_CLICKS on the mini window's root): Allow on hub's row answers
// its ask, a quick phrase sends to the lead, and the review folded into Cue's bubble unfolds.
test("mini: the chat's buttons work as in Cue's window", async ({ cue, page }) => {
  await openLead(cue, page, "rail");
  await showCrew(page, true);
  await page.locator('.m-chat .mt-row.need [data-act="allow"]').click();
  await cue.called("respond", { id: "n1793984390004" });
  await showCrew(page, false);
  await page.locator(".m-chat .cue-you", { hasText: "Asked for handoff's review" }).locator('[data-ask-open$=":review"]').click();
  await page.locator(".m-chat .cue-you .cv-voice-line", { hasText: "Recommendation" }).waitFor();
  await page.locator('.m-chat .quick .qp[data-phrase="Go ahead"]').click();
  await cue.called("send_to_session", { sessionId: "lead", text: "Go ahead" });
  // Its box is Cue's: Enter sends what's typed; Esc folds the chat back into the rail's tab.
  await page.locator('.m-chat textarea[data-text="s:lead"]').fill("and run the tests");
  await page.keyboard.press("Enter");
  await cue.called("send_to_session", { sessionId: "lead", text: "and run the tests" });
  await page.keyboard.press("Escape");
  await page.locator("#mini .m-tuck").waitFor();
});

// 📎 in the box opens Finder's picker, which takes the window's focus: the chat stays open (it folds on a blur
// otherwise), and the picture lands in its box.
test("mini: a picture picked from the box lands in it", async ({ cue, page }) => {
  await openLead(cue, page, "rail");
  const chooser = page.waitForEvent("filechooser");
  await page.locator('.m-chat [data-attach="s:lead"]').click();
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await page.waitForTimeout(400);   // longer than the chat takes to fold (mini.js closeChat)
  await expect(page.locator(".m-chat")).toHaveCount(1);
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64");
  await (await chooser).setFiles({ name: "shot.png", mimeType: "image/png", buffer: png });
  await page.locator('.m-chat .chip img[alt="shot.png"]').waitFor();
  await expect(page.locator(".m-chat")).toHaveCount(1);
  // Picked: a blur folds it again, as a click outside does.
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await page.locator("#mini .m-tuck").waitFor();
});

// The picker shut with no word of it (no change, no cancel: a panel dismissed in some builds): the window's focus back
// ends the pick, and the next blur folds the chat as ever.
test("mini: a picker that closes without a word doesn't keep the chat open", async ({ cue, page }) => {
  await openLead(cue, page, "rail");
  // Playwright holds the panel: it never sends change or cancel.
  const chooser = page.waitForEvent("filechooser");
  await page.locator('.m-chat [data-attach="s:lead"]').click();
  await chooser;
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await page.waitForTimeout(400);   // longer than the chat takes to fold (mini.js closeChat)
  await expect(page.locator(".m-chat")).toHaveCount(1);
  await page.evaluate(() => { window.dispatchEvent(new Event("focus")); window.dispatchEvent(new Event("blur")); });
  await page.locator("#mini .m-tuck").waitFor();
});
