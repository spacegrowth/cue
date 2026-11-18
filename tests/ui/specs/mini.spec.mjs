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

// ---------- the Desk at the notch and on the rail (#239) ----------
// desk.json's Desks, made fuller: the cue Desk named, a permission a session's item waits on, and on the website Desk
// a question its holder ended its turn on; plus a session on no Desk, at work.
function deskWorld(s) {
  const cue = s.desk.projects.find((p) => p.project === "cue"), web = s.desk.projects.find((p) => p.project === "website");
  cue.name = "Cue Desk";
  const at = s.now_ms;
  cue.items.push({ ...cue.items.find((i) => i.id === "d5"), id: "d11", state: "needs_you", title: "Run the test suite before merging", created: at - 300000, updated: at - 60000,
    ask: { item_id: "n-perm", sid: "host", kind: "permission", text: "host-lead wants to run: npm test" } });
  s.items.push({ ...s.items.find((i) => i.id === "n-focus-ask"), id: "n-perm", kind: "permission", session_id: "host", agent_id: "", agent_name: "", tool_name: "Bash", tool_input: { command: "npm test" }, created_ms: at - 60000 });
  web.items.push({ ...web.items.find((i) => i.id === "w1"), id: "w3", state: "needs_you", title: "Logo on the launch page", created: at - 900000, updated: at - 120000 });
  const site = s.sessions.find((x) => x.session_id === "site");
  site.thread.push({ role: "agent", text: "Should the launch page keep the old logo until the new one is ready?", at_ms: at - 120000 });
  s.sessions.push({ ...site, session_id: "notes", name: "release-notes", project: "notes", state: "working", since_ms: at - 240000, thread: [] });
}
async function openDesk(cue, page, shape, { frozen = false, dark } = {}) {
  await page.setViewportSize(WINDOW[shape]);
  await page.addInitScript((h) => Object.defineProperty(Screen.prototype, "height", { get: () => h }), SCREEN.height);
  if (dark !== undefined) await page.emulateMedia({ colorScheme: dark ? "dark" : "light" });
  await cue.openMini("desk", shape, { frozen });
  await cue.change(deskWorld);
}
/** The pointer over the notch, or the rail's tab pressed: it opens on the Desk with the most waiting. */
async function openDeskTab(page, shape) {
  if (shape === "notch") await page.evaluate(() => document.body.dispatchEvent(new MouseEvent("mouseenter")));
  else await page.locator("[data-out]").click();
  await page.locator(".m-chat .m-dk").waitFor();
}

test("mini: the notch's island counts the Desk's items once, its tab first", async ({ cue, page }) => {
  await openDesk(cue, page, "notch");
  // Three wait on you across the Desks (d1, d11 on Cue Desk; w3 on website). The reviews stay out. Their sessions'
  // own cards (n-focus-ask, n-perm) aren't counted again beside it.
  const left = page.locator(".m-island .m-wing").first();
  await expect(left.locator(".m-dk-n")).toHaveText("3");
  await expect(left.locator(".m-n")).toHaveCount(1);
  await expect(left.locator(".m-dk-n .dk-kic")).toHaveCount(1);
  // The wing fits in the island: nothing runs past its edge.
  const box = await page.locator(".m-island").boundingBox(), wing = await left.boundingBox();
  expect(wing.x).toBeGreaterThanOrEqual(box.x);
  // Hover opens on the Desk with the most waiting, its tab before the website's and the sessions'.
  await openDeskTab(page, "notch");
  const tabs = page.locator(".m-tabs .m-tab");
  await expect(tabs.nth(0)).toHaveAttribute("data-mopen", "desk:cue");
  await expect(tabs.nth(0)).toHaveClass(/\bon\b/);
  await expect(tabs.nth(0)).toHaveText("Cue Desk2");
  await expect(tabs.nth(1)).toHaveAttribute("data-mopen", "desk:website");
  // A session on no Desk has a tab of its own, with no Desk mark.
  await expect(page.locator('.m-tab[data-mopen="notes"]')).toHaveCount(1);
  await expect(page.locator('.m-tab[data-mopen="notes"] .dk-kic')).toHaveCount(0);
  await expect(page.locator(".m-chat-head .m-cnt")).toHaveText(["2", "2", "2"]);
  // ⤢ Open Desk: the Desk in Cue's window.
  await page.locator(".m-chat-head [data-mdesk]").click();
  await cue.called("mini_open_in_cue", { desk: "cue" });
});

test("mini: a Desk's rows answer in place", async ({ cue, page }) => {
  await openDesk(cue, page, "notch");
  await openDeskTab(page, "notch");
  const row = (id) => page.locator(`.m-dk-it[data-mdesk-item="${id}"]`);
  await expect(page.locator(".m-dk-g")).toHaveText(["Waiting on you2", "Ready to review1", "In progress2"]);
  // Its holder: the lead → its crew member.
  await expect(row("d1").locator(".m-dk-who")).toHaveText("web-lead→focus-fix");
  await expect(row("d1").locator(".m-dk-who .dk-kic")).toHaveCount(2);
  // In progress: its rows, no ask under them; the queued one counted under them.
  await expect(row("d3").locator(".m-dk-ask")).toHaveCount(0);
  await expect(row("d5")).toHaveCount(1);
  await expect(page.locator(".m-dk-rest")).toHaveText("2 queued");
  // A question's own answer, typed: Answer opens a box in the row, Enter sends it as the card's answer.
  await expect(row("d1").locator("[data-pick]")).toHaveText(["Keep it in the box", "Move to the next item"]);
  await row("d1").locator("[data-mdk-answer]").click();
  await row("d1").locator("textarea[data-mdk-box]").fill("Keep it, and select the text");
  await page.keyboard.press("Enter");
  await cue.called("respond", { id: "n-focus-ask", decision: { behavior: "allow", answers: { "Keep focus in the box, or move to the next item?": "Keep it, and select the text" } } });
  await expect(row("d1").locator(".m-dk-said")).toHaveText("✓ You answered: Keep it, and select the text");
  // A permission: Allow in the row.
  await row("d11").locator('[data-desk-decision="allow"]').click();
  await cue.called("respond", { id: "n-perm", decision: { behavior: "allow" } });
  // Ready to check: Looks right in the row, "Done · Undo" for a moment, then it goes.
  await row("d2").locator('[data-desk-act="looks_right"]').click();
  await expect(row("d2").locator(".dk-soon")).toHaveCount(1);
  await page.clock.runFor(3200);
  await cue.called("desk_looks_right", { id: "d2" });
  // A row opens its item on the Desk in Cue.
  await row("d3").locator(".m-dk-t").click();
  await cue.called("mini_open_in_cue", { desk: "cue", item: "d3" });
});

test("mini: a question its holder ended on, answered from the row", async ({ cue, page }) => {
  await openDesk(cue, page, "notch");
  await openDeskTab(page, "notch");
  await page.locator('.m-tab[data-mopen="desk:website"]').click();
  const row = page.locator('.m-dk-it[data-mdesk-item="w3"]');
  await expect(row.locator(".m-dk-q")).toHaveText("Should the launch page keep the old logo until the new one is ready?");
  // No choices: Answer only, then the box; Send sends to its session about the item, as from its page on the Desk.
  await expect(row.locator(".m-dk-acts button")).toHaveText(["Answer"]);
  await row.locator("[data-mdk-answer]").click();
  await row.locator("textarea[data-mdk-box]").fill("Keep the old one for now");
  await row.locator("[data-mdk-send]").click();
  await cue.called("send_to_session", { sessionId: "site", text: "About #w3 (Logo on the launch page): Keep the old one for now" });
  await expect(row.locator(".m-dk-said")).toHaveText("✓ You answered: Keep the old one for now");
  // Send back on the one to check: its own box in the row, Enter sends it back.
  const w1 = page.locator('.m-dk-it[data-mdesk-item="w1"]');
  await w1.locator('[data-desk-act="send_back"]').click();
  await w1.locator('textarea[data-text="back:w1"]').fill("The header is cut off");
  await page.keyboard.press("Enter");
  await cue.called("desk_item_act", { id: "w1", act: "send_back", text: "The header is cut off" });
});

// The rail wears Cue's theme: its Desk tab in light and in dark, the same rows.
for (const dark of [false, true]) {
  test(`mini: the rail's Desk tab, ${dark ? "dark" : "light"}`, async ({ cue, page }) => {
    await openDesk(cue, page, "rail", { dark });
    await openDeskTab(page, "rail");
    await expect(page.locator(".m-bar .m-tab").first()).toHaveAttribute("data-mopen", "desk:cue");
    await expect(page.locator(".m-dk-cnts .m-cnt")).toHaveText(["2", "2", "2"]);
    const bg = await page.locator(".m-chat").evaluate((e) => getComputedStyle(e).backgroundColor);
    const ink = await page.locator(".m-dk-t").first().evaluate((e) => getComputedStyle(e).color);
    const lum = (c) => { const [r, g, b] = c.match(/[\d.]+/g).map(Number); return 0.3 * r + 0.59 * g + 0.11 * b; };
    expect(dark ? lum(bg) < lum(ink) : lum(bg) > lum(ink), `${dark ? "dark" : "light"} panel, ink that reads on it`).toBe(true);
    // Nothing runs past the panel's edge.
    expect(await page.locator(".m-dk").evaluate((e) => e.scrollWidth <= e.clientWidth)).toBe(true);
  });
}

test("mini: the tabs are five at most, the Desks' first", async ({ cue, page }) => {
  await openDesk(cue, page, "notch");
  // Four more sessions at work, on no Desk: with the two Desks' tabs, more than five would want a tab.
  await cue.change((s) => {
    const notes = s.sessions.find((x) => x.session_id === "notes");
    for (const n of [1, 2, 3, 4]) s.sessions.push({ ...notes, session_id: `more${n}`, name: `more-${n}`, since_ms: s.now_ms - n * 1000 });
  });
  await openDeskTab(page, "notch");
  const tabs = page.locator(".m-tabs .m-tab");
  await expect(tabs).toHaveCount(5);
  await expect(tabs.nth(0)).toHaveAttribute("data-mopen", "desk:cue");
  await expect(tabs.nth(1)).toHaveAttribute("data-mopen", "desk:website");
});

test("mini: a Desk with nothing waiting or at work has no tab; hover opens on a session", async ({ cue, page }) => {
  await openDesk(cue, page, "notch");
  // Every item on both Desks done: nothing there for the notch.
  await cue.change((s) => { for (const p of s.desk.projects) for (const i of p.items) { i.state = "done"; delete i.ask; } });
  await expect(page.locator(".m-island .m-dk-n")).toHaveCount(0);
  await page.evaluate(() => document.body.dispatchEvent(new MouseEvent("mouseenter")));
  await page.locator(".m-chat .m-tabs .m-tab.on").waitFor();
  await expect(page.locator(".m-tabs .m-dtab")).toHaveCount(0);
  await expect(page.locator(".m-tabs .m-tab.on")).not.toHaveAttribute("data-mopen", /^desk:/);
  await expect(page.locator(".m-chat .m-dk")).toHaveCount(0);
});

test("mini: an open Desk tab that empties keeps its tab; one that goes away gives way to another", async ({ cue, page }) => {
  await openDesk(cue, page, "notch");
  await openDeskTab(page, "notch");
  await page.locator('.m-tab[data-mopen="desk:website"]').click();
  await cue.change((s) => { for (const i of s.desk.projects.find((p) => p.project === "website").items) { i.state = "done"; delete i.ask; } });
  await expect(page.locator('.m-tab.on[data-mopen="desk:website"]')).toHaveCount(1);
  await expect(page.locator(".m-dk-it")).toHaveCount(0);
  await cue.change((s) => { s.desk.projects = s.desk.projects.filter((p) => p.project !== "website"); });
  await expect(page.locator('.m-tab[data-mopen="desk:website"]')).toHaveCount(0);
  await expect(page.locator(".m-tabs .m-tab.on")).toHaveCount(1);
});
