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

  test("the Cue Crew chip at the top right opens its two settings, and each goes to the lead", async ({ cue, page }) => {
    await expect(page.locator(".foot .team-sw")).toHaveCount(0);
    await page.locator('.ap-head [data-crew-menu="lead"]').click();
    await page.locator('[data-team-sw="lead"][data-team-mode="off"]').click();
    expect((await cue.called("team_mode", { sessionId: "lead" })).args.mode).toBe("off");
    await expect(page.locator(".team-sw button.on").first()).toHaveText("Never");
    await expect(page.locator('.ap-head [data-crew-menu="lead"]')).toContainText("Cue Crew: never");
    await expect(page.locator(".team-sw").nth(1).locator("button.on")).toHaveText("Handle it");
    await page.locator('[data-crew-commit="lead"][data-crew-mode="wait"]').click();
    expect((await cue.called("crew_commit", { sessionId: "lead" })).args.mode).toBe("wait");
    await expect(page.locator(".crew-foot")).toContainText("waits for your Commit or Send fixes");
    await page.locator(".ap-chat").first().click({ position: { x: 20, y: 20 } });
    await expect(page.locator(".crew-menu")).toHaveCount(0);
  });

  test("the crew is a drawer on the box: its title bar is the tab, opened; Close, the bar, the tab and Esc toggle it", async ({ page }) => {
    // Opened by itself: hub asked and handoff is ready (they wait on you).
    const drawer = page.locator(".crew-drawer");
    await expect(drawer).toBeVisible();
    await expect(drawer.locator(".crew-bar")).toHaveText(/^Cue Crew · 5 · 2 working · 2 ready\s*Close ▾$/);
    await expect(drawer.locator(".crew-bar .dot-live"), "a live dot while any member works").toHaveCount(1);
    await expect(page.locator(".crew-tab"), "the tab isn't shown while its drawer is open").toBeHidden();
    expect(await page.locator(".ap-chat").first().evaluate((el) => getComputedStyle(el).opacity)).not.toBe("1");
    const box = await page.locator(".foot .field").boundingBox(), d = await drawer.boundingBox();
    expect(Math.abs(d.y + d.height - box.y), "it stands on the box's top edge").toBeLessThan(2);
    await expect(drawer.locator("[title]"), "no hover text").toHaveCount(0);
    await drawer.locator(".crew-close").click();
    await expect(drawer).toHaveCount(0);
    await expect(page.locator(".crew-tab")).toBeVisible();
    await expect(page.locator(".crew-tab")).toHaveText(/^Cue Crew · 5 · 2 working · 2 ready▴$/);
    await page.locator(".crew-tab").click();
    await expect(drawer).toBeVisible();
    await drawer.locator(".crew-bar-w").click();
    await expect(drawer, "a click on its title bar closes it").toHaveCount(0);
    await page.locator(".crew-tab").click();
    // Esc goes to a box with text in it; an empty box (or none focused) gives it to the drawer.
    const text = page.locator('textarea[data-text="s:lead"]');
    await text.fill("hello");
    await text.press("Escape");
    await expect(drawer).toBeVisible();
    await text.fill("");
    await text.press("Escape");
    await expect(drawer).toHaveCount(0);
    await page.locator(".crew-tab").click();
    await page.locator(".ap-chat").first().click({ position: { x: 20, y: 20 } });
    await page.keyboard.press("Escape");
    await expect(drawer).toHaveCount(0);
  });

  test("open, the chat's newest message sits just above the drawer; closed, at the chat's end, nothing jumps", async ({ page }) => {
    const drawer = page.locator(".crew-drawer");
    await expect(drawer).toBeVisible();
    // The chat's last message's bottom, the drawer's top, the chat's bottom, and whether the chat is at its end.
    const where = () => page.evaluate(() => {
      const chat = document.querySelector(".active-pane .ap-chat"), last = [...chat.children].filter((e) => e.offsetHeight).at(-1);
      const d = document.querySelector(".crew-drawer");
      return { last: last.getBoundingClientRect().bottom, drawer: d ? d.getBoundingClientRect().top : null, chat: chat.getBoundingClientRect().bottom, end: chat.scrollHeight - chat.scrollTop - chat.clientHeight < 2 };
    });
    const open = await where();
    expect(open.end, "the chat at its end").toBe(true);
    expect(open.last, "its newest message isn't under the drawer").toBeLessThanOrEqual(open.drawer);
    expect(open.drawer - open.last, "just above it").toBeLessThan(24);
    await drawer.locator(".crew-close").click();
    await expect(drawer).toHaveCount(0);
    const closed = await where();
    expect(closed.end, "closed: still at its end").toBe(true);
    expect(closed.chat - closed.last, "its newest message at the chat's foot, no gap left where the drawer was").toBeLessThan(24);
    await page.locator(".crew-tab").click();
    await expect(drawer).toBeVisible();
    expect(await where(), "open again: just where it was").toEqual(open);
  });

  test("the drawer opens by itself once when a member starts waiting on you; closed, it stays closed", async ({ cue, page }) => {
    const drawer = page.locator(".crew-drawer");
    await drawer.locator(".crew-close").click();
    await cue.change((st) => { const lead = st.sessions.find((s) => s.session_id === "lead"); lead.team.find((t) => t.name === "rows").step = "Editing ui/a.js"; });
    await expect(page.locator(".crew-tab")).toBeVisible();
    await expect(drawer, "nothing new waits: stays closed").toHaveCount(0);
    await cue.change((st) => { const lead = st.sessions.find((s) => s.session_id === "lead"); lead.team.find((t) => t.name === "skill").state = "reviewed"; });
    await expect(drawer, "skill is ready: open again").toBeVisible();
  });

  test("no Set aside anywhere; files Cue never saw: Commit; what the hub says it needs shows as needs you", async ({ cue, page }) => {
    await cue.change((st) => {
      const lead = st.sessions.find((s) => s.session_id === "lead"), at = (n) => lead.team.find((t) => t.name === n);
      Object.assign(at("skill"), { files: [], stat: "", worktree: "" });
      Object.assign(at("rows"), { state: "done", needs: "Its lead didn't say it committed it" });
    });
    const drawer = page.locator(".crew-drawer");
    const skill = drawer.locator(".mt-row", { hasText: "skill" });
    await expect(skill.locator(".mt-line")).toContainText("Cue doesn't know which files it changed: Commit asks its lead to commit them");
    await expect(skill.locator("[data-mt-act]")).toHaveText(["Commit"]);
    const rows = drawer.locator(".mt-row", { hasText: "rows" }).first();
    await expect(rows.locator(".tm-chip.need")).toHaveText("needs you");
    await expect(rows.locator(".mt-line")).toContainText("Its lead didn't say it committed it");
    await expect(drawer.locator(".crew-bar")).toContainText("1 needs you");
    for (const more of await drawer.locator("[data-mt-more]").all()) {
      await more.click();
      await expect(page.locator(".mt-menu")).toBeVisible();
      await expect(page.locator(".mt-menu")).not.toContainText("Set aside");
      await page.keyboard.press("Escape");
    }
    await expect(page.locator(".active-pane"), "no Set aside anywhere").not.toContainText("Set aside");
    await skill.locator('[data-mt-act="commit"]').click();
    expect((await cue.called("team_action", { action: "commit" })).args).toMatchObject({ sessionId: "lead", agent: "id-skill" });
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

  test("a crew member's Diff opens the lead's Diff tab on its files, and ✕ goes back to the whole diff", async ({ cue, page }) => {
    const crew = page.locator("[data-mt-open]").first();
    if ((await crew.getAttribute("aria-expanded")) !== "true") await crew.click();
    await expect(page.locator('.mt-row [data-mt-act="stop"]').first()).toBeVisible();
    await expect(page.locator('[data-mt-act="diff"][data-mt-agent="id-hub"]'), "hub has no files Cue knows: no Diff").toHaveCount(0);
    await page.locator('[data-mt-act="diff"][data-mt-agent="id-skill"]').click();
    expect((await cue.called("team_action", { action: "diff" })).args.agent).toBe("id-skill");
    await expect(page.locator(".stab.on")).toHaveText("Diff");
    await expect(page.locator(".dv-chip")).toHaveText(/^skill's 2 files/);
    await expect(page.locator(".dv-file .dv-path")).toHaveText(["skills/team/SKILL.md", "skills/team/review.md"]);
    await expect(page.locator(".dv-repo"), "one repo: no repo on each file").toHaveCount(0);
    await expect(page.locator(".mt-diff")).toHaveCount(0);
    await page.locator("[data-dv-all]").click();
    await expect(page.locator(".dv-chip")).toHaveCount(0);
    await expect(page.locator(".dv-seg")).toBeVisible();
    await expect(page.locator(".dv-file")).toHaveCount(0);
    // Chat, and the Diff tab goes: the lead has no changes of its own.
    await page.locator('.stab[data-stab=""]').click();
    await expect(page.locator('.stab[data-stab=":diff"]')).toHaveCount(0);
  });

  test("a review waiting for the lead is Cue's bubble too, and Skip takes it off the queue", async ({ cue, page }) => {
    await cue.change((st) => { st.review_queue = { lead: [{ exec: "id-skill", name: "skill" }] }; });
    const waits = page.locator(".cue-you", { hasText: "Will ask for skill's review" });
    await expect(waits).toHaveCount(1);
    await expect(waits.locator(".cv-you-text b")).toHaveText("skill");
    await expect(waits.locator(".cv-meta")).toHaveText("Cue · waits for the lead to be free · Skip");
    await expect(waits.locator("[data-ask-open]")).toHaveCount(0);
    // The crew's drawer (open by itself) stands over the chat's foot: put it away to reach the bubble.
    await page.locator(".crew-drawer .crew-close").click();
    await waits.locator('[data-act="unqueue-review"]').click();
    expect((await cue.called("unqueue_review")).args.exec).toBe("id-skill");
    await expect(waits).toHaveCount(0);
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
