// The window on states a real hub built (tests/ui/scenarios/world-*.json, testing/scenarios.rs): no sessions, one
// ask answered by its keys (A, D, R, and 1–4 on a question), fifty sessions, long names, a restart under way, and
// what Park and an ended session leave on the shelves. Each checks what the window sent, or what it shows.
import { test, expect } from "./cue.mjs";
import { layoutProblems, LINE, NARROWEST } from "./layout.mjs";

/** The card id every one-card world gives its card (the World's frozen clock, plus one). */
const ONLY = "n1793984400001";
const card = (page, id) => page.locator(`.nrow[data-big="${id}"]`);

test("with no sessions the board says nothing waits and lists nothing", async ({ cue, page }) => {
  await cue.open("world-empty");
  await expect(page.locator(".nrow")).toHaveCount(0);
  await expect(page.locator(".sec-waiting")).toContainText("Nothing waiting.");
  await expect(page.locator(".sec-sessions")).toContainText("No other sessions.");
});

test.describe("one ask, answered by its keys", () => {
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("world-one-ask");
    await expect(page.locator(".active-pane")).toContainText("rm -rf build && cargo build");
  });

  test("A allows it and its card goes", async ({ cue, page }) => {
    await page.keyboard.press("a");
    expect((await cue.called("respond", { id: ONLY })).args.decision).toEqual({ behavior: "allow" });
    await expect(card(page, ONLY)).toHaveCount(0);
  });

  test("D denies it", async ({ cue, page }) => {
    await page.keyboard.press("d");
    expect((await cue.called("respond", { id: ONLY })).args.decision).toEqual({ behavior: "deny" });
    expect(await cue.calls("respond")).toHaveLength(1);
  });

  test("R, then what to do instead, denies with it", async ({ cue, page }) => {
    await page.keyboard.press("r");
    const box = page.locator(`.active-pane textarea[data-text="${ONLY}"]`);
    await expect(box).toBeFocused();
    await box.pressSequentially("run cargo clean instead");
    await box.press("Enter");
    expect((await cue.called("respond", { id: ONLY })).args.decision).toEqual({ behavior: "deny", message: "run cargo clean instead" });
  });

  test("a number key on a permission answers nothing", async ({ cue, page }) => {
    await page.keyboard.press("1");
    await page.keyboard.press("2");
    await expect(card(page, ONLY)).toBeVisible();
    expect(await cue.calls("respond")).toEqual([]);
  });
});

test.describe("a question with four choices", () => {
  const CHOICES = ["Vendor", "Exchange", "Newest", "Ask me each time"];
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("world-four-choices");
    await expect(page.locator(".active-pane")).toContainText("Which source wins on a clash?");
  });

  for (const [i, choice] of CHOICES.entries()) {
    test(`${i + 1} picks “${choice}”`, async ({ cue, page }) => {
      await page.keyboard.press(String(i + 1));
      expect((await cue.called("respond", { id: ONLY })).args.decision).toEqual({ behavior: "allow", answers: { "Which source wins on a clash?": choice } });
    });
  }

  test("5, past its choices, picks nothing", async ({ cue, page }) => {
    await page.keyboard.press("5");
    await expect(card(page, ONLY)).toBeVisible();
    expect(await cue.calls("respond")).toEqual([]);
  });
});

test("fifty sessions: each listed once, the seventeen finished as cards and the rest at work", async ({ cue, page }) => {
  await cue.open("world-fifty-sessions");
  await expect(page.locator(".sec-waiting .nrow")).toHaveCount(50);
  await expect(page.locator(".sec-waiting .nrow[data-big]")).toHaveCount(17);
  await expect(page.locator(".sec-waiting .nrow.talk-work[data-session]")).toHaveCount(33);
  const ids = await page.evaluate(() => [...document.querySelectorAll(".nrow[data-session]")].map((e) => e.dataset.session));
  expect(new Set(ids).size, "no session twice").toBe(ids.length);
});

test("a turn its relay lead drove, and a session out of usage, aren't cards for you", async ({ cue, page }) => {
  await cue.open("world-many-sessions");
  await expect(page.locator(".sec-waiting .nrow[data-big]")).toHaveCount(4);
  await expect(page.locator(".sec-waiting")).not.toContainText("Report written.");
  await expect(page.locator(".sec-waiting")).not.toContainText("session limit");
});

for (const [size, viewport] of [["", null], [" at the narrowest window", NARROWEST]]) {
  test(`a long name and folder stay inside their row${size}`, async ({ cue, page }) => {
    if (viewport) await page.setViewportSize(viewport);
    await cue.open("world-long-names", { frozen: true });
    await expect(page.locator('.nrow[data-session="long"]')).toBeVisible();
    expect(await layoutProblems(page, ".sec-waiting .nrow-top", LINE(".nrow"))).toEqual([]);
    const overflow = await page.evaluate(() => [...document.querySelectorAll(".nrow")].filter((r) => r.scrollWidth > r.clientWidth + 0.5).length);
    expect(overflow, "rows that scroll sideways").toBe(0);
  });
}

test("a session restarting shows its steps where its pane was", async ({ cue, page }) => {
  await cue.open("world-restarting");
  const pane = page.locator(".active-pane");
  await expect(pane).toContainText("Restarting api-server");
  await expect(pane).toContainText("Load 1 MCP server");
});

test("a parked session is under Parked with no card left, an ended one under Closed", async ({ cue, page }) => {
  await cue.open("world-parked");
  await expect(page.locator(".nrow")).toHaveCount(0);
  await page.locator('[data-shelf="parked"]').click();
  await expect(page.locator('.prow.parked[data-session="old"]')).toBeVisible();
  await page.locator('[data-shelf="closed"]').click();
  await expect(page.locator('.prow.closed[data-session="gone"]')).toBeVisible();
});
