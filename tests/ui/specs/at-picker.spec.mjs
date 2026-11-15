// "@" in a lead's box (chat/compose.js atMatches): its crew above the box, each with its state in a word, narrowing
// as you type; ↑ / ↓ move, Enter, Tab or a click put "@name " in the box (nothing is sent), Esc closes it and keeps
// the draft. What's typed goes to the lead as it is; the lead passes it on.
import { test, expect } from "./cue.mjs";

const TEAM_ASK = "n1793984390004";
const leadBox = (page) => page.locator('.active-pane textarea[data-text="s:lead"]');
const menu = (page) => page.locator(".at-menu");
const rows = (page) => menu(page).locator(".cmd-row");

test.beforeEach(async ({ cue, page }) => {
  await cue.open("world-team");
  await page.locator(`.nrow[data-big="${TEAM_ASK}"]`).click();
  await expect(leadBox(page)).toBeVisible();
});

test("@ lists the crew with each one's state; typing narrows it; ↓ and Enter put the name in, nothing sent", async ({ cue, page }) => {
  const box = leadBox(page);
  await box.click();
  await box.pressSequentially("ask @");
  // The ones still in the crew first, the set-aside one last.
  await expect(rows(page).locator("b")).toHaveText(["@hub", "@rows", "@skill", "@handoff", "@docs", "@spike"]);
  await expect(rows(page).first().locator("i"), "hub's ask waits on you").toHaveText("needs you");
  await expect(rows(page).first().locator("i")).toHaveClass("need");
  await expect(rows(page).nth(1).locator("i")).toHaveText("checking");
  await expect(rows(page).nth(5).locator("i")).toHaveText("set aside");
  await expect(rows(page).first()).toHaveClass(/\bon\b/);
  await page.screenshot({ path: "/tmp/at-picker.png" });
  // "d": names that start with it, then names that hold it.
  await box.pressSequentially("d");
  await expect(rows(page).locator("b")).toHaveText(["@docs", "@handoff"]);
  await box.press("ArrowDown");
  await expect(rows(page).nth(1)).toHaveClass(/\bon\b/);
  await box.press("Enter");
  await expect(box).toHaveValue("ask @handoff ");
  await expect(menu(page)).toHaveCount(0);
  await expect(box).toBeFocused();
  expect(await box.evaluate((el) => el.selectionStart), "the caret after the name").toBe(13);
  expect(await cue.calls("send_to_session"), "picking a name sends nothing").toHaveLength(0);
});

test("Tab and a click pick too; a name typed out in full closes it; no menu for an @ inside a word", async ({ page }) => {
  const box = leadBox(page);
  await box.click();
  await box.pressSequentially("@ro");
  await expect(rows(page).locator("b")).toHaveText(["@rows"]);
  await box.press("Tab");
  await expect(box).toHaveValue("@rows ");
  await box.pressSequentially("and @sk");
  await rows(page).first().click();
  await expect(box).toHaveValue("@rows and @skill ");
  await box.pressSequentially("@docs");
  await expect(menu(page), "typed out in full: nothing left to pick").toHaveCount(0);
  await box.pressSequentially(" mail me@h");
  await expect(menu(page), "an @ inside a word is not a name").toHaveCount(0);
});

test("Esc closes it and keeps what's typed; typing again opens it", async ({ page }) => {
  const box = leadBox(page);
  await box.click();
  await box.pressSequentially("tell @h");
  await expect(rows(page).locator("b")).toHaveText(["@hub", "@handoff"]);
  await box.press("Escape");
  await expect(menu(page)).toHaveCount(0);
  await expect(box).toHaveValue("tell @h");
  await expect(box).toBeFocused();
  await box.pressSequentially("u");
  await expect(rows(page).locator("b")).toHaveText(["@hub"]);
});

test("the crew page's box has it too", async ({ page }) => {
  await page.locator('.crew-tray [data-cp-open="lead"]').click();
  const box = page.locator('.crew-page textarea[data-text="s:lead"]');
  await box.click();
  await box.pressSequentially("@sp");
  await expect(rows(page).locator("b")).toHaveText(["@spike"]);
  await box.press("Enter");
  await expect(box).toHaveValue("@spike ");
});

test("a session with no crew: @ opens nothing", async ({ page }) => {
  await page.evaluate(() => { sessionOf("lead").team = []; });
  const box = leadBox(page);
  await box.click();
  await box.pressSequentially("@");
  await expect(page.locator(".cmd-slot .cmd-menu")).toHaveCount(0);
});
