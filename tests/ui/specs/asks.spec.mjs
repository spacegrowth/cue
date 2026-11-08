// Answering what waits: permission asks (buttons and the A / D / R keys), questions (options and number keys),
// and finished turns set aside. Each checks what the window sent (respond, dismiss, set_later) and that the card
// leaves Waiting once Cue says it's answered.
import { test, expect } from "./cue.mjs";

/** A card in Waiting, by its item id. */
const card = (page, id) => page.locator(`.nrow[data-big="${id}"]`);

test.beforeEach(async ({ cue }) => { await cue.open("board-waiting"); });

test("Allow on a permission card allows it", async ({ cue, page }) => {
  await card(page, "n5").locator('[data-act="allow"]').click();
  const c = await cue.called("respond", { id: "n5" });
  expect(c.args.decision).toEqual({ behavior: "allow" });
  await expect(card(page, "n5")).toHaveCount(0);
});

test("Deny on a permission card denies it", async ({ cue, page }) => {
  await card(page, "n4").locator('[data-act="deny"]').click();
  expect((await cue.called("respond", { id: "n4" })).args.decision).toEqual({ behavior: "deny" });
});

test("A allows the ask that's open", async ({ cue, page }) => {
  await card(page, "n4").click();
  await expect(page.locator(".active-pane")).toContainText("uv run pytest tests/lines -q -x");
  await page.keyboard.press("a");
  expect((await cue.called("respond", { id: "n4" })).args.decision.behavior).toBe("allow");
  expect(await cue.calls("respond")).toHaveLength(1);
});

test("D denies the ask that's open", async ({ cue, page }) => {
  await card(page, "n5").click();
  await expect(page.locator(".active-pane")).toContainText("Cart.tsx");
  await page.keyboard.press("d");
  expect((await cue.called("respond", { id: "n5" })).args.decision.behavior).toBe("deny");
});

test("R, then a message, denies with what to do instead", async ({ cue, page }) => {
  await card(page, "n4").click();
  await page.keyboard.press("r");
  const box = page.locator('.active-pane textarea[data-text="n4"]');
  await expect(box).toBeFocused();
  await box.pressSequentially("run only the failing test");
  await box.press("Enter");
  expect((await cue.called("respond", { id: "n4" })).args.decision).toEqual({ behavior: "deny", message: "run only the failing test" });
});

test("A question's option on its card answers it", async ({ cue, page }) => {
  await card(page, "n6").locator('[data-pick="0:1"]').click();
  const c = await cue.called("respond", { id: "n6" });
  expect(c.args.decision).toEqual({ behavior: "allow", answers: { "Which cache should the quotes endpoint use?": "In-process LRU" } });
});

test("A number key picks that option of the open question", async ({ cue, page }) => {
  await expect(page.locator(".active-pane")).toContainText("Which cache should the quotes endpoint use?");
  await page.keyboard.press("3");
  expect((await cue.called("respond", { id: "n6" })).args.decision.answers).toEqual({ "Which cache should the quotes endpoint use?": "No cache" });
});

test("A, with nothing to allow open (a question), sends nothing", async ({ cue, page }) => {
  await expect(page.locator(".active-pane")).toContainText("Which cache should the quotes endpoint use?");
  await page.keyboard.press("a");
  await page.keyboard.press("d");
  await expect(card(page, "n6")).toBeVisible();
  expect(await cue.calls("respond")).toEqual([]);
});

test("× on a finished turn dismisses it", async ({ cue, page }) => {
  await card(page, "n3").locator('[data-act="dismiss"]').click();
  await cue.called("dismiss", { id: "n3" });
  await expect(card(page, "n3")).toHaveCount(0);
});

test("Later moves a finished turn to Need to decide", async ({ cue, page }) => {
  await card(page, "n2").locator("[data-park]").click();
  await cue.called("set_later", { sessionId: "det", on: true });
});

test("A plan to approve shows the plan as written, and Approve plan allows it", async ({ cue, page }) => {
  await cue.change((st) => {
    const it = st.items.find((x) => x.id === "n5");
    it.tool_name = "ExitPlanMode";
    it.tool_input = { plan: "# Ship the cart limit\n\n- Raise MAX_ITEMS to 25\n- Add a test", planFilePath: "/x/plan.md" };
  });
  const c = card(page, "n5");
  await expect(c).toContainText("has a plan for you");
  await expect(c).toContainText("Plan: Ship the cart limit");
  await c.click();
  const pane = page.locator(".active-pane");
  await expect(pane.locator(".plan-body li")).toHaveCount(2);
  await expect(pane).not.toContainText("planFilePath");
  await expect(pane.locator('[data-act="deny"]')).toHaveText("Keep planning");
  await pane.locator('[data-act="allow"]').click();
  expect((await cue.called("respond", { id: "n5" })).args.decision).toEqual({ behavior: "allow" });
});

test("Indented points under a numbered item are a list of their own, not more numbers", async ({ page }) => {
  const html = await page.evaluate(() => md("1. Shape: fine.\n2. Assumptions:\n   - one\n   - two\n3. Verify: ok."));
  const box = await page.evaluate((h) => { const d = document.createElement("div"); d.innerHTML = h; return { top: [...d.querySelectorAll(":scope > ol > li")].map((li) => li.firstChild.textContent.trim()), sub: [...d.querySelectorAll("ol > li > ul > li")].map((li) => li.textContent) }; }, html);
  expect(box.top).toEqual(["Shape: fine.", "Assumptions:", "Verify: ok."]);
  expect(box.sub).toEqual(["one", "two"]);
});
