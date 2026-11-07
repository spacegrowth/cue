// The rest of the board: relay's review cards, parking and resuming, stars, the project switcher, search, and
// Settings switches. Each checks the call the window made and what it shows once Cue's state comes back.
import { test, expect } from "./cue.mjs";
import { compareShot } from "./shots.mjs";

const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();

test.describe("relay's review cards", () => {
  test.beforeEach(async ({ cue, page }) => { await cue.open("relay-review"); await openCard(page, "n1"); });
  /** The lead's review row about executor `name`. */
  const rv = (page, name) => page.locator(".active-pane .rv-row").filter({ hasText: name });

  test("Commit types \"commit it\" into the lead", async ({ cue, page }) => {
    await rv(page, "tz-fix").getByRole("button", { name: "Commit" }).click();
    expect((await cue.called("send_to_session", { sessionId: "relay" })).args).toMatchObject({ text: "commit it", now: false });
  });

  test("Send fixes types \"send the fixes\" into the lead", async ({ cue, page }) => {
    await rv(page, "tz-fix").getByRole("button", { name: "Send fixes" }).click();
    expect((await cue.called("send_to_session", { sessionId: "relay" })).args.text).toBe("send the fixes");
  });

  test("Review asks relay to review that executor's packet", async ({ cue, page }) => {
    await rv(page, "export-csv").getByRole("button", { name: "Review" }).click();
    await cue.called("crew_action", { sessionId: "x-miss", action: "review" });
  });

  test("Set aside (in the row's ⋯ menu) sets that packet aside", async ({ cue, page }) => {
    await rv(page, "export-csv").locator("[data-rv-more]").click();
    await page.locator('.rv-menu [data-crew="aside"]').click();
    await cue.called("crew_action", { sessionId: "x-miss", action: "aside" });
  });
});

test.describe("parking", () => {
  test("⋯ → Park parks the open session and it moves under Parked", async ({ cue, page }) => {
    await cue.open("board-waiting");
    await openCard(page, "n1");
    await page.locator('.active-pane [data-more="relay"]').click();
    await page.locator('[data-park-session="relay"]').click();
    await cue.called("park_session", { sessionId: "relay" });
    await expect(page.locator('.nrow[data-big="n1"]')).toHaveCount(0);
    await page.locator('[data-shelf="parked"]').click();
    await expect(page.locator('.prow.parked[data-session="relay"]')).toBeVisible();
  });

  test("↺ resumes a parked session and it's live again", async ({ cue, page }) => {
    await cue.open("parked-ended");
    await page.locator('[data-shelf="parked"]').click();
    await page.locator('[data-wake="guide"]').click();
    await cue.called("resume_parked", { sessionId: "guide" });
    await expect(page.locator('.prow.parked[data-session="guide"]')).toHaveCount(0);
    expect(await page.evaluate(() => state.sessions.some((s) => s.session_id === "guide"))).toBe(true);
  });

  test("↺ on a closed session resumes it", async ({ cue, page }) => {
    await cue.open("parked-ended");
    await page.locator('[data-shelf="closed"]').click();
    await page.locator('[data-wake="lost1"]').click();
    await cue.called("resume_closed", { sessionId: "lost1" });
  });
});

test.describe("the board", () => {
  test.beforeEach(async ({ cue }) => { await cue.open("board-waiting"); });

  test("☆ stars a session, and it's listed under Starred", async ({ cue, page }) => {
    await page.locator('.nrow[data-big="n3"] [data-star="sec"]').click();
    await cue.called("set_starred", { sessionId: "sec", on: true });
    await expect(page.locator('.nrow[data-big="n3"] [data-star="sec"]')).toHaveAttribute("aria-pressed", "true");
  });

  test("the project switcher shows one project's sessions only", async ({ page }) => {
    await page.locator('[data-pj="toggle"]').click();
    await page.locator('.pj-pop [data-pj="pick"][data-project="agent-relay"]').click();
    await expect(page.locator(".pj-open").first()).toContainText("agent-relay");
    await expect(page.locator(".sec-waiting .nrow")).toHaveCount(1);
    await expect(page.locator('.nrow[data-big="n1"]')).toBeVisible();
  });

  test("/ searches everything Cue logged, and a found session opens", async ({ cue, page }) => {
    await page.keyboard.press("/");
    const find = page.locator('[data-text="find-live"]');
    await expect(find).toBeFocused();
    await expect(find).toHaveValue("");
    await find.pressSequentially("coupon");
    await cue.called("search", { q: "coupon" });
    await expect(page.getByText("checkout-flow").first()).toBeVisible();
  });

  // The window is drawn whole on every change: a bar an element a stretch made a busy board's bars most of
  // each draw (core.js bar). Its stretches are one masked layer a kind, and the one still going.
  test("a session's bar is a few elements however many stretches it had", async ({ cue, page }) => {
    await cue.change((st) => {
      const s = st.sessions.find((x) => x.session_id === "relay"), t = Date.now() - 1_800_000;
      s.segments = Array.from({ length: 300 }, (_, k) => ({ kind: k % 2 ? "working" : "waiting", start_ms: t + k * 5_990, end_ms: t + (k + 1) * 5_990 }));
      s.segments.push({ kind: "waiting", start_ms: t + 300 * 5_990, end_ms: null });
    });
    await openCard(page, "n1");
    const bar = page.locator(".ap-bar .bar");
    await expect(bar.locator("i")).toHaveCount(3);
    await expect(bar.locator("i.live")).toHaveCount(1);
    const spans = (cls) => bar.locator(`i.layer.${cls}`).evaluate((el) => ((el.style.maskImage || el.style.webkitMaskImage).match(/#000|rgb\(0, 0, 0\)/g) || []).length);
    expect(await spans("w")).toBe(300);   // each stretch is two opaque stops
    expect(await spans("z")).toBe(300);
  });
});

test.describe("Settings", () => {
  test.beforeEach(async ({ cue, page }) => { await cue.open("board-waiting"); await page.keyboard.press(","); });

  test("a switch saves its setting", async ({ cue, page }) => {
    await page.locator('.sheet [data-set="appearance.project"]').click();
    await cue.called("set_setting", { key: "appearance.project", value: true });
    await expect(page.locator('.sheet [data-set="appearance.project"]')).toHaveAttribute("aria-checked", "true");
  });

  test("Dark saves the appearance and the window turns dark", async ({ cue, page }) => {
    await page.locator('.sheet [data-set="appearance.mode"][data-val=\'"dark"\']').click();
    await cue.called("set_setting", { key: "appearance.mode", value: "dark" });
    await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme || document.documentElement.className)).toMatch(/dark/);
  });

  test("a tab's own setting saves under its key", async ({ cue, page }) => {
    await page.locator('[data-act="set-tab"][data-tab="summaries"]').click();
    await page.locator('.sheet [data-set="tasks.on"]').click();
    await cue.called("set_setting", { key: "tasks.on", value: true });
  });

  test("Get beta builds is off at first, saves its setting and looks again on beta", async ({ cue, page }) => {
    const beta = page.locator('.sheet [data-set="updates.beta"]');
    await expect(beta).toHaveAttribute("aria-checked", "false");
    await expect(page.locator(".sheet")).toContainText("You have the latest version.");
    await beta.click();
    await cue.called("set_setting", { key: "updates.beta", value: true });
    await expect(beta).toHaveAttribute("aria-checked", "true");
    await expect(page.locator(".sheet")).toContainText("Cue 0.1.87 is out. A beta build.");
    expect(await cue.calls("update_check")).toHaveLength(2);
  });

  test("Get beta builds with no beta out says the answer is stable's", async ({ cue, page }) => {
    await cue.open("board-waiting", { flags: "nobeta=1" });
    await page.keyboard.press(",");
    await page.locator('.sheet [data-set="updates.beta"]').click();
    await expect(page.locator(".sheet")).toContainText("You have the latest version. No beta is out right now, so this is the version everyone gets.");
  });

  test("Esc closes Settings", async ({ page }) => {
    await expect(page.locator(".sheet")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator(".sheet")).toHaveCount(0);
  });
});

// A waiting stretch wide enough for its stripes: striped, each one outlined at both its ends (core.js barLayerStyle's
// `--ends` over the layer's own top and bottom), as drawn.
test("a wide waiting stretch is striped and outlined at both its ends", async ({ cue, page }) => {
  await cue.open("board-waiting", { frozen: true });
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "relay"), t = Date.now() - 1_800_000, m = 60_000;
    const at = (kind, a, b) => ({ kind, start_ms: t + a * m, end_ms: b == null ? null : t + b * m });
    s.segments = [at("working", 0, 4), at("waiting", 4, 11), at("working", 11, 14), at("waiting", 14, 24), at("working", 24, 26), at("waiting", 26, 26.3), at("working", 26.3, null)];
  });
  await openCard(page, "n1");
  const striped = page.locator(".ap-bar .bar i.layer.z:not(.thin)");
  await expect(striped).toHaveCount(1);
  await expect(page.locator(".ap-bar .bar i.layer.z.thin")).toHaveCount(1);
  // Two stretches, each end a 1 px line of two stops.
  expect(((await striped.evaluate((el) => el.style.getPropertyValue("--ends"))).match(/var\(--accent\)/g) || []).length).toBe(8);
  expect(await striped.evaluate((el) => getComputedStyle(el).backgroundImage), "the ends drawn over the stripes").toMatch(/^linear-gradient\(.*repeating-linear-gradient/);
  await page.mouse.move(0, 0);
  const r = compareShot("bar-wide-waiting", await page.locator(".ap-bar").screenshot({ animations: "disabled" }));
  expect(r.baseline, "baseline for bar-wide-waiting").not.toBe("missing");
  expect(r.differ, "bar-wide-waiting: pixels that differ from tests/ui/baseline/bar-wide-waiting.png").toBe(0);
});
