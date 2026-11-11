// The rest of the board: parking and resuming, stars, the project switcher, search, and
// Settings switches. Each checks the call the window made and what it shows once Cue's state comes back.
import { test, expect } from "./cue.mjs";
import { compareShot } from "./shots.mjs";

const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();

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

// How work goes out to a hosted lead's team (ui/team_send.js; world-team: lead "lead", its teammate hub asking).
test.describe("a hosted lead's crew: the Crew switch, /crew, a task dropped on the lead", () => {
  const TEAM_ASK = "n1793984390004";
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("world-team");
    await openCard(page, TEAM_ASK);
    await page.locator('.act-card [data-act="allow"]').click();
  });
  const drop = (page, text) => page.evaluate((text) => {
    const dt = new DataTransfer();
    dt.setData("text/plain", text);
    const el = document.querySelector(".working[data-session='lead']");
    for (const type of ["dragover", "drop"]) el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: dt }));
  }, text);

  test("the Teammates chip at the top right opens its two settings, and each goes to the lead", async ({ cue, page }) => {
    await expect(page.locator(".foot .team-sw")).toHaveCount(0);
    await page.locator('.ap-head [data-crew-menu="lead"]').click();
    await page.locator('[data-team-sw="lead"][data-team-mode="off"]').click();
    expect((await cue.called("team_mode", { sessionId: "lead" })).args.mode).toBe("off");
    await expect(page.locator(".team-sw button.on").first()).toHaveText("Never");
    await expect(page.locator('.ap-head [data-crew-menu="lead"]')).toContainText("Teammates: never");
    await expect(page.locator(".team-sw").nth(1).locator("button.on")).toHaveText("Handle it");
    await page.locator('[data-crew-commit="lead"][data-crew-mode="wait"]').click();
    expect((await cue.called("crew_commit", { sessionId: "lead" })).args.mode).toBe("wait");
    await expect(page.locator(".crew-foot")).toContainText("waits for your Commit or Send fixes");
    await page.locator(".ap-chat").first().click();
    await expect(page.locator(".crew-menu")).toHaveCount(0);
  });

  test("Cue's review request is Cue's bubble on the right, the review one line, and Open › shows its findings under its row", async ({ page }) => {
    await expect(page.locator(".cv-you:not(.cue-you)", { hasText: "Review your teammate" })).toHaveCount(0);
    const asked = page.locator(".cue-you", { hasText: "Asked for handoff's review" });
    await expect(asked).toHaveCount(1);
    await expect(asked.locator(".cv-meta")).toHaveText(/^Cue · /);
    await expect(asked.locator(".cv-voice-line")).toHaveCount(0);
    await asked.locator("[data-ask-open]").click();
    await expect(asked.locator("[data-ask-open]")).toHaveText("− What Cue sent");
    await expect(asked.locator(".cv-voice-line", { hasText: "Review your teammate handoff's work" })).toHaveCount(1);
    const line = page.locator(".cv-review", { hasText: "handoff reviewed" });
    await line.locator("[data-crew-open]").click();
    await expect(page.locator(".crew-dock .mt-under", { hasText: "hub/handoff.rs:40" })).toBeVisible();
  });

  test("/crew is one of the lead's commands, in Cue", async ({ page }) => {
    await page.locator('textarea[data-text="s:lead"]').fill("/cre");
    await expect(page.locator('.cmd-row[data-cmd-name="crew"]')).toContainText("in Cue");
  });

  test("a task that names a file a teammate changes says so, and Send anyway hands it out", async ({ cue, page }) => {
    await drop(page, "Fix ui/team.js and ui/board.js");
    await expect(page.locator(".ho .ho-f")).toHaveCount(2);
    await expect(page.locator(".ho .ho-by")).toHaveText("rows is changing it");
    await page.locator("[data-ho-go]", { hasText: "Send anyway" }).click();
    expect((await cue.called("team_hand_out", { sessionId: "lead" })).args.text).toBe("Fix ui/team.js and ui/board.js");
    await expect(page.locator(".ho")).toHaveCount(0);
  });

  test("a task with nothing taken is Hand out; Cancel drops it", async ({ cue, page }) => {
    await drop(page, "Add a line to notes/c.txt");
    await expect(page.locator("[data-ho-go]")).toHaveText("Hand out");
    await page.locator("[data-ho-cancel]").click();
    await expect(page.locator(".ho")).toHaveCount(0);
    expect(await cue.calls("team_hand_out")).toHaveLength(0);
  });
});
