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

// A crew member's ask is answered on its row in the crew's tray, not in the ask panel (world-team: hub asks).
test("a crew member's ask: on its row under Needs you, with ↪, Deny and Allow; the box stays; the tray opens for it", async ({ cue, page }) => {
  await cue.open("world-team");
  await page.locator('.nrow[data-big="n1793984390004"]').click();
  const tray = page.locator(".crew-tray");
  await expect(page.locator(".act-card"), "no ask panel of its own").toHaveCount(0);
  await expect(page.locator('textarea[data-text="s:lead"]'), "the lead's box stays").toBeVisible();
  await expect(tray.locator(".crew-head")).toContainText("Cue Crew · 5 · hub needs you · 2 reviewed · 1 working");
  await expect(tray.locator(".mt-grp").first()).toHaveText("Needs you · 1");
  const hub = tray.locator(".mt-row.need");
  await expect(hub.locator(".mt-n")).toHaveText("hubwants to run");
  await expect(hub.locator(".mt-btns button")).toHaveText(["↪", "Deny", "Allow"]);
  await expect(hub.locator(".mt-ask")).toContainText("cd src-tauri && cargo test --features ext");
  // ↪: its box under the ask, as on the ask's own card.
  await hub.locator('[data-act="redirect"]').click();
  await expect(hub.locator('.mt-ask textarea[data-text="n1793984390004"]')).toBeVisible();
  // Closed by you, opening its card again doesn't reopen it; a new ask does.
  await tray.locator(".crew-car").click();
  await expect(tray.locator(".crew-body")).toHaveCount(0);
  await tray.locator(".crew-car").click();
  await tray.locator('.mt-row.need [data-act="allow"]').click();
  expect((await cue.called("respond", { id: "n1793984390004" })).args.decision.behavior).toBe("allow");
  await expect(tray.locator(".mt-row.need")).toHaveCount(0);
});

test("a crew member's ask open while the lead asks too: the lead's own card stands under the chat, and its keys are that card's", async ({ cue, page }) => {
  await cue.open("world-team");
  await cue.change((st) => {
    const hub = st.items.find((i) => i.id === "n1793984390004");
    st.items.push({ ...hub, id: "own1", agent_id: "", agent_name: "", agent_pid: 0, tool_input: { command: "git push" }, created_ms: hub.created_ms + 1 });
  });
  await page.locator('.nrow[data-big="n1793984390004"]').click();
  await expect(page.locator(".act-card"), "the lead's own ask has its card").toContainText("git push");
  await expect(page.locator(".act-card [data-act='allow']")).toHaveAttribute("data-id", "own1");
  await expect(page.locator(".crew-tray .mt-row.need"), "hub's ask stays on its row").toContainText("cargo test --features ext");
  await page.locator(".ap-chat").first().click({ position: { x: 20, y: 20 } });
  await page.keyboard.press("a");
  expect((await cue.called("respond", { id: "own1" })).args.decision.behavior).toBe("allow");
  expect(await cue.calls("respond"), "A answered the card shown, not hub's").toHaveLength(1);
  await expect(page.locator(".act-card")).toHaveCount(0);
  await expect(page.locator('textarea[data-text="s:lead"]'), "answered: the box is back, hub's ask still on its row").toBeVisible();
  await expect(page.locator(".crew-tray .mt-row.need")).toHaveCount(1);
});

// The crew's page (ui/team_page.js): Open full › on the tray's line puts it across the window, in the board's and the
// chat's place; ‹ Back to the board, Open chat › and Esc go back to the chat; its box is the chat's (the same draft),
// Talk types "@name " in it; the lead's replies and its decisions in one stream, newest last.
test.describe("a hosted lead's crew on its own page", () => {
  const TEAM_ASK = "n1793984390004";
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("world-team");
    await openCard(page, TEAM_ASK);
    await page.locator('.crew-tray [data-cp-open="lead"]').click();
    await expect(page.locator('.crew-page[data-crew-page="lead"]')).toBeVisible();
  });

  test("Open full opens it across the window; Back to the board returns to the chat, its draft carried over", async ({ page }) => {
    await expect(page.locator(".active-pane, .col-waiting, .col-sessions"), "the page in the chat's and the columns' place").toHaveCount(0);
    await expect(page.locator(".switch-view"), "the tabs stay").toBeVisible();
    const cp = await page.locator(".crew-page").boundingBox(), vw = page.viewportSize();
    expect(cp.width, "full width").toBeGreaterThan(vw.width - 80);
    expect(await page.evaluate(() => { const b = document.querySelector(".cp-body"); return b.scrollHeight - b.clientHeight; }), "fits the window: nothing to scroll").toBeLessThanOrEqual(1);
    await expect(page.locator(".crew-page .cp-lanes > .cp-lane"), "the members still in the crew").toHaveCount(5);
    // The set-aside one folded at the end, as in the drawer; unfolded in place, dimmed.
    const done = page.locator(".crew-page .cp-done .mt-done-line");
    await expect(done).toHaveText("1 done ›");
    await expect(page.locator(".crew-page .cp-lane.past")).toHaveCount(0);
    await done.click();
    await expect(page.locator(".crew-page .cp-done .cp-lane.past")).toHaveCount(1);
    await expect(done).toHaveText("1 done ⌄");
    await expect(page.locator(".cp-status .mt-grp")).toHaveText("The crew · 6");
    await expect(page.locator(".cp-status [data-crew-menu]")).toHaveText("Handle it ▾");
    await expect(page.locator(".cp-needs.red .mt-row.need [data-act=allow]")).toBeVisible();
    // The goal on one line; All of it › opens the four boxes in place.
    await expect(page.locator(".cp-goal .cp-gt")).toContainText("Done well when: Each part reviewed");
    await page.locator("[data-cp-goal]").click();
    await expect(page.locator(".cp-goal.open .cg-box")).toHaveCount(4);
    await page.locator("[data-cp-goal]").click();
    await expect(page.locator(".cp-goal .cg-box")).toHaveCount(0);
    // One stream: the lead's replies and its decisions in time order, each decision with Go back on this.
    const stream = page.locator(".cp-stream");
    const decided = await page.evaluate(() => sessionOf("lead").crew_log.length);
    await expect(stream.locator(".cp-sh")).toContainText(`newest last · ${decided} decided`);
    await expect(stream.locator(".cd-row.big")).toHaveCount(decided);
    await expect(stream.locator(".cd-row.big [data-act=crew-back]").first()).toHaveText("Go back on this");
    expect(await stream.locator(".cv-agent").count(), "the lead's replies").toBeGreaterThan(0);
    const order = await stream.locator(".cp-items-in > *").evaluateAll((els) => els.map((e) => e.dataset.key));
    const at = (k) => +(k.match(/^[ay]:(\d+)$/)?.[1] ?? NaN);
    const said = order.filter((k) => /^[ay]:/.test(k)).map(at);
    expect(said, "newest last").toEqual([...said].sort((a, b) => a - b));
    // Its box is the chat's: what's typed there is there when you go back.
    await stream.locator('textarea[data-text="s:lead"]').fill("carry on");
    await expect(page.locator("[data-cp-back]")).toHaveText("‹ Back to the board");
    await page.locator("[data-cp-back]").click();
    await expect(page.locator(".crew-page")).toHaveCount(0);
    await expect(page.locator(".col-waiting"), "the board's columns are back").toBeVisible();
    await expect(page.locator('.active-pane textarea[data-text="s:lead"]'), "the chat as it was, the draft in its box").toHaveValue("carry on");
  });

  test("Esc goes back to the chat; Talk types @name in the stream's box; Open chat goes back too", async ({ page }) => {
    await page.locator('.cp-lane[data-key="cp:lead|id-rows"] [data-cp-talk]').click();
    const box = page.locator('.crew-page textarea[data-text="s:lead"]');
    await expect(box).toBeFocused();
    await expect(box).toHaveValue("@rows ");
    await box.fill("");
    await page.keyboard.press("Escape");
    await expect(page.locator(".crew-page")).toHaveCount(0);
    await expect(page.locator(".active-pane .crew-tray")).toBeVisible();
    await page.locator('.crew-tray [data-cp-open="lead"]').click();
    await page.locator("[data-cp-chat]").click();
    await expect(page.locator(".crew-page")).toHaveCount(0);
    await expect(page.locator(".active-pane")).toBeVisible();
  });

  test("the stream shows what the chat does: the reply the lead stopped on when it waits on you, and a message queued in it", async ({ cue, page }) => {
    const stream = page.locator(".cp-stream .cp-items");
    await cue.change((st) => {
      const lead = st.sessions.find((s) => s.session_id === "lead");
      lead.state = "waiting";
      st.items = st.items.filter((i) => i.session_id !== "lead");
      st.items.push({ id: "w-lead", kind: "waiting", harness: "claude", session_id: "lead", cwd: lead.cwd, project: lead.project, message: "All five are in: docs is committed, the rest wait on review. Your turn.", created_ms: lead.thread.at(-1).at_ms + 1000 });
    });
    await expect(stream.locator(".cv-agent").last(), "its last reply, from its waiting card").toContainText("All five are in: docs is committed");
    await cue.change((st) => {
      const lead = st.sessions.find((s) => s.session_id === "lead");
      lead.state = "working";
      lead.queued = { role: "you", text: "and keep spike out of it", at_ms: lead.thread.at(-1).at_ms + 2000, images: [] };
    });
    await expect(stream.locator(".cv-you.queued").last(), "what you queued, newest last").toContainText("and keep spike out of it");
  });

  test("a lane's steps are its own call's: docs started again under its name shows none of the old docs's", async ({ cue, page }) => {
    const lane = page.locator('.cp-lane[data-key="cp:lead|id-docs"]');
    await lane.locator("[data-cp-lane]").click();
    await expect(lane.locator(".co-body"), "docs's log entry, by its call").toContainText("Reviewed: 1 note");
    await cue.change((st) => { Object.assign(st.sessions.find((s) => s.session_id === "lead").team.find((t) => t.name === "docs"), { call: "call-docs-2", state: "working", findings: [], recommendation: "", step: "Editing docs" }); });
    await expect(lane.locator(".co-body")).not.toContainText("Reviewed: 1 note");
    await expect(lane.locator(".co-st.now")).toHaveText(/Editing docs/);
  });

  test("its lanes read down in columns: who with its meter, what it was asked, what it does now, its work; a name opens its lane in place", async ({ page }) => {
    await expect(page.locator(".cp-hd")).toHaveText(/Who\s*What it was asked\s*Now\s*Its work/i);
    const hub = page.locator('.cp-lane[data-key="cp:lead|id-hub"]'), rows = page.locator('.cp-lane[data-key="cp:lead|id-rows"]');
    await expect(hub.locator(".cp-st")).toHaveText("asks you");
    await expect(hub.locator(".cp-ml")).toHaveText("asked you");
    await expect(hub.locator(".cp-m b.need")).toHaveCount(1);
    await expect(hub.locator(".cp-ask")).toHaveText(/^Keep a team per hosted lead/);
    await expect(hub.locator(".cp-big.q")).toHaveText("Wants to run cd src-tauri && cargo test --features ext");
    await expect(hub.locator("[data-cp-answer]"), "Answer when it asks you").toHaveText("Answer");
    await expect(rows.locator(".cp-big")).toHaveText("Running node tests/team.test.js");
    await expect(rows.locator(".cp-ml"), "a test run is checking").toHaveText("checking");
    await expect(rows.locator(".cp-work")).toContainText("2 files so far");
    // Columns line up down the page.
    const x = (sel) => page.locator(`.cp-lanes > .cp-lane ${sel}`).evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().left)));
    for (const sel of [".cp-ask", ".cp-now", ".cp-work"]) expect(new Set(await x(sel)).size, sel).toBe(1);
    // Answer takes you to its row in Needs you.
    await hub.locator("[data-cp-answer]").click();
    await expect(page.locator('.cp-needs [data-key="mt:lead|id-hub"] button:not(.mt-name)').first()).toBeFocused();
    // Opening a lane: in place, others stay put; its steps with times, what review found and with what came of it,
    // its files.
    const order = () => page.locator(".crew-page .cp-lanes > .cp-lane").evaluateAll((els) => els.map((e) => e.dataset.key));
    const before = await order(), handoff = page.locator('.cp-lane[data-key="cp:lead|id-handoff"]');
    await handoff.locator("[data-cp-lane]").click();
    await expect(handoff).toHaveClass(/open/);
    expect(await order()).toEqual(before);
    await expect(handoff.locator(".co-st").filter({ hasText: /^\d+:\d\dStarted: Carry the team over a handoff$/ })).toHaveCount(1);
    await expect(handoff.locator(".co-st").filter({ hasText: "Reviewed: 1 should-fix, 1 note" })).toHaveCount(1);
    await expect(handoff.locator(".co-fd")).toHaveCount(2);
    await expect(handoff.locator(".co-fd").first().locator(".co-fs")).toHaveText("sent back");
    await expect(handoff.locator(".co-fl")).toHaveCount(2);
    // Open, its task stays at two lines (a lead's brief can be pages long): the steps lead. A short task gets no
    // Full task; a long one does, and it opens the rest in place.
    await expect(handoff.locator(".cp-ask")).not.toHaveClass(/full/);
    await expect(handoff.locator("[data-cp-ask]")).toHaveCount(0);
    await page.evaluate(() => { const t = sessionOf("lead").team.find((m) => m.name === "handoff"); t.asked = "Carry the team over a handoff. ".repeat(12); renderMain(); });
    await handoff.locator("[data-cp-ask]").click();
    await expect(handoff.locator(".cp-ask")).toHaveClass(/full/);
    await handoff.locator("[data-cp-ask]").click();
    await expect(handoff.locator(".cp-ask")).not.toHaveClass(/full/);
    await expect(handoff.locator("[data-cp-lane]")).toContainText("⌄");
    await handoff.locator("[data-cp-lane]").click();
    await expect(handoff.locator(".co-body")).toHaveCount(0);
    expect(await page.locator(".crew-page .cp-lanes [title]").count(), "no hover text on the lanes").toBe(0);
  });

  test("a long reply in the lead's stream is cut at whole blocks, never through a table; Show all opens it", async ({ cue, page }) => {
    const rowsMd = Array.from({ length: 12 }, (_, i) => `| part ${i} | done |`).join("\n");
    await cue.change((st, rowsMd) => {
      const lead = st.sessions.find((s) => s.session_id === "lead");
      lead.thread.push({ role: "agent", text: `Where the crew is:\n\n| part | state |\n|---|---|\n${rowsMd}\n\nThat's all.`, at_ms: lead.thread.at(-1).at_ms + 500, images: [] });
    }, rowsMd);
    const reply = page.locator(".cp-items .cv-agent").filter({ hasText: "Where the crew is" });
    await expect(reply.locator(".cv-text.cut")).toBeVisible();
    await expect(reply.locator(".cv-text table"), "the table left out whole, not sliced").toHaveCount(0);
    await expect(reply.locator(".cp-more")).toHaveText("Show all ›");
    await reply.locator(".cp-more").click();
    await expect(reply.locator(".cv-text tbody tr")).toHaveCount(12);
    await expect(reply.locator(".cv-text")).toContainText("That's all.");
    await expect(reply.locator(".cp-more")).toHaveText("Less ‹");
  });

  test("the lead's stream and Needs you get 45% of the page at least; what's above scrolls inside itself", async ({ page }) => {
    const sizes = () => page.evaluate(() => {
      const h = (sel) => document.querySelector(sel).getBoundingClientRect().height, u = document.querySelector(".cp-upper");
      return { body: h(".cp-body"), split: h(".cp-split"), upper: u.scrollHeight > u.clientHeight + 1 };
    });
    for (const vp of [{ width: 1280, height: 800 }, { width: 1280, height: 1000 }, { width: 900, height: 600 }]) {
      await page.setViewportSize(vp);
      // Every lane open (the done ones too) so the top has more than its share.
      if (!(await page.locator(".cp-done.open").count())) await page.locator(".cp-done .mt-done-line").click();
      const z = await sizes();
      expect(z.split, `${vp.width}×${vp.height}: the bottom's share`).toBeGreaterThanOrEqual(Math.min(0.45 * z.body, z.body) - 1);
      expect(await page.evaluate(() => { const b = document.querySelector(".cp-body"); return b.scrollHeight - b.clientHeight; }), `${vp.width}×${vp.height}: the page itself doesn't scroll`).toBeLessThanOrEqual(1);
    }
  });

  test("its lanes: what needs you first, then those at work, then the rest, each in the crew's order; a click moves none", async ({ cue, page }) => {
    const order = () => page.locator(".crew-page .cp-lanes > .cp-lane").evaluateAll((els) => els.map((e) => e.dataset.key.split("|id-")[1]));
    // hub asks you; rows works; skill done, handoff and docs reviewed, in the crew's order.
    await expect.poll(order).toEqual(["hub", "rows", "skill", "handoff", "docs"]);
    // The tab stays Board: the page opened from the board shows within it.
    await expect(page.locator(".switch-view button.on")).toHaveText("Board");
    // docs goes back to work: it joins those at work, after rows (the crew's order).
    await cue.change((st) => { Object.assign(st.sessions.find((s) => s.session_id === "lead").team.find((t) => t.name === "docs"), { state: "working", step: "Editing docs" }); });
    await expect.poll(order).toEqual(["hub", "rows", "docs", "skill", "handoff"]);
    // handoff needs you: it joins hub at the top.
    await cue.change((st) => { st.sessions.find((s) => s.session_id === "lead").team.find((t) => t.name === "handoff").needs = "commit"; });
    await expect.poll(order).toEqual(["hub", "handoff", "rows", "docs", "skill"]);
    // Opening a lane's menu, its review or its Diff moves nothing.
    await page.locator('.cp-lane[data-key="cp:lead|id-skill"] [data-cp-lane]').click();
    expect(await order()).toEqual(["hub", "handoff", "rows", "docs", "skill"]);
  });

  test("it closes when its lead goes away", async ({ cue, page }) => {
    await cue.change((st) => { st.sessions = st.sessions.filter((s) => s.session_id !== "lead"); st.items = st.items.filter((i) => i.session_id !== "lead"); });
    await expect(page.locator(".crew-page")).toHaveCount(0);
  });
});

// How work goes out to a hosted lead's team (ui/team_send.js; world-team: lead "lead", its teammate hub asking).
test.describe("a hosted lead's crew: the Crew switch, /crew, a task dropped on the lead", () => {
  const TEAM_ASK = "n1793984390004";
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("world-team");
    await openCard(page, TEAM_ASK);
    await page.locator('.mt-row.need [data-act="allow"]').click();
  });
  /** Ask me: every review waits under Ready with its buttons (on Handle it, the default, Cue handles them itself). */
  const askMe = (cue) => cue.change((st) => { st.sessions.find((s) => s.session_id === "lead").crew_commit = "wait"; });
  // The lead's card under Sessions is drawn once its ask is answered (as team-handout waits for it).
  const drop = async (page, text) => (await page.locator(".working[data-session='lead']").waitFor(), page.evaluate((text) => {
    const dt = new DataTransfer();
    dt.setData("text/plain", text);
    const el = document.querySelector(".working[data-session='lead']");
    for (const type of ["dragover", "drop"]) el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: dt }));
  }, text));

  test("the mode on the tray's line opens its two settings, and each goes to the lead; no chip in the head", async ({ cue, page }) => {
    await expect(page.locator(".ap-head [data-crew-menu]"), "no crew chip in the head").toHaveCount(0);
    await expect(page.locator(".foot .team-sw")).toHaveCount(0);
    const tray = page.locator(".crew-tray");
    await tray.locator(".crew-car").click();
    await expect(tray.locator(".crew-body"), "closed").toHaveCount(0);
    await expect(tray.locator('[data-crew-menu="lead"]'), "the mode shows on the closed line too").toHaveText("Handle it ▾");
    await tray.locator('[data-crew-menu="lead"]').click();
    await expect(tray.locator(".crew-body"), "the mode doesn't open the tray").toHaveCount(0);
    await page.locator('[data-team-sw="lead"][data-team-mode="off"]').click();
    expect((await cue.called("team_mode", { sessionId: "lead" })).args.mode).toBe("off");
    await expect(page.locator(".team-sw button.on").first()).toHaveText("Never");
    await expect(page.locator(".team-sw").nth(1).locator("button.on"), "Review: only code in a repo, the default").toHaveText("Only code in a repo");
    await expect(page.locator(".crew-foot")).toContainText("Fixes go back once");
    await page.locator('[data-crew-review="lead"][data-crew-mode="never"]').click();
    expect((await cue.called("crew_review", { sessionId: "lead" })).args.mode).toBe("never");
    await expect(page.locator(".team-sw").nth(1).locator("button.on")).toHaveText("Never");
    await expect(page.locator(".crew-foot")).toContainText("Review is in each member's menu.");
    await expect(page.locator(".team-sw").nth(2).locator("button.on")).toHaveText("Handle it");
    await page.locator('[data-crew-commit="lead"][data-crew-mode="wait"]').click();
    expect((await cue.called("crew_commit", { sessionId: "lead" })).args.mode).toBe("wait");
    await expect(page.locator(".crew-foot")).toContainText("waits for your Commit or Send fixes");
    await expect(tray.locator('[data-crew-menu="lead"]')).toHaveText("Ask me ▾");
    const menu = await page.locator(".crew-menu").boundingBox(), line = await tray.locator(".crew-head").boundingBox();
    expect(menu.y + menu.height, "it opens upward, over the chat").toBeLessThanOrEqual(line.y);
    await page.locator(".ap-chat").first().click({ position: { x: 20, y: 20 } });
    await expect(page.locator(".crew-menu")).toHaveCount(0);
  });

  // Handle it (the default): Cue sends fixes and commits by itself, so no Send fixes or Commit; what it decided is
  // listed, each line with Go back on this, which asks the lead and leaves the line saying "going back…".
  test("Handle it: the goal on top, what it decided on its own, and Go back on this", async ({ cue, page }) => {
    const tray = page.locator(".crew-tray");
    await expect(tray.locator(".crew-body")).toBeVisible();
    await expect(tray.locator(".cg-box .cg-t")).toHaveText(["Build the team view: hand out hub, rows, skill and handoff", "Each part reviewed, with nothing blocking", "Review, send fixes, commit to crew/ branches", "Merge to main, real choices"]);
    await expect(tray.locator(".mt-grp")).toHaveText(["Needs you · nothing now", "Decided on its own · 5", "Working"]);
    await expect(tray.locator('[data-mt-act="fixes"], [data-mt-act="commit"]'), "Cue does those itself").toHaveCount(0);
    await expect(tray.locator(".mt-one", { hasText: "handoff" }).locator(".mt-line")).toContainText("reviewed · Cue sends its fixes back");
    await expect(tray.locator(".cd-row .cd-who")).toHaveText(["docs", "handoff", "rows", "spike", "lead"]);
    const docs = tray.locator(".cd-row", { hasText: "docs" });
    await expect(docs.locator(".cd-what code")).toHaveText("crew/docs");
    // Its line opens in place: its review's findings, the commit and its Diff.
    await docs.locator(".cd-what").click();
    await expect(tray.locator(".cd-under")).toContainText("Committed 3f2a1c9");
    await expect(tray.locator('.cd-under [data-mt-act="diff"]')).toHaveText("Diff");
    await docs.locator(".cd-what").click();
    await expect(tray.locator(".cd-under")).toHaveCount(0);
    await docs.locator('[data-act="crew-back"]').click();
    const id = (await cue.called("crew_go_back", { sessionId: "lead" })).args.id;
    expect(id).toMatch(/^d\d+-0$/);
    await expect(docs.locator(".cd-going")).toHaveText("going back…");
    await expect(docs.locator('[data-act="crew-back"]')).toHaveCount(0);
    await expect(tray.locator(".cd-under"), "the button doesn't open its line").toHaveCount(0);
    await expect(page.locator(".toast")).toHaveText("Asked the lead to go back on it: docs");
    await expect(tray.locator("[title]"), "no hover text").toHaveCount(0);
  });

  test("the crew is a tray on the box: one edge with it; its line, its caret and Esc toggle it", async ({ cue, page }) => {
    // Opened by itself on Ask me: handoff and docs are ready (they wait on you).
    await askMe(cue);
    const tray = page.locator(".crew-tray");
    await expect(tray.locator(".crew-body")).toBeVisible();
    await expect(tray.locator(".crew-head")).toHaveText(/^Cue Crew · 5 · 2 ready · 2 working\s*Open full ›\s*Ask me ▾\s*▾$/);
    await expect(tray.locator(".crew-head .dot-live"), "a live dot while any member works").toHaveCount(1);
    await expect(tray.locator(".mt-grp")).toHaveText(["Needs you · nothing now", "Ready", "Decided on its own · 5", "Working"]);
    const box = await page.locator(".foot .field").boundingBox(), t = await tray.boundingBox();
    expect(Math.abs(t.y + t.height - box.y), "it stands on the box's top edge").toBeLessThan(2);
    expect(Math.abs(t.x - box.x) + Math.abs(t.width - box.width), "the box's width").toBeLessThan(2);
    expect(await page.locator(".foot .field").evaluate((el) => getComputedStyle(el).borderTopLeftRadius), "the box's top corners go square").toBe("0px");
    await expect(tray.locator("[title]"), "no hover text").toHaveCount(0);
    // Every row's button one width, so they line up.
    const widths = await tray.locator(".mt-btns .mt-b").evaluateAll((els) => [...new Set(els.map((e) => Math.round(e.getBoundingClientRect().width)))]);
    expect(widths).toHaveLength(1);
    await tray.locator(".crew-car").click();
    await expect(tray.locator(".crew-body")).toHaveCount(0);
    await expect(tray.locator(".crew-head")).toHaveText(/^Cue Crew · 5 · 2 ready · 2 working\s*Open full ›\s*Ask me ▾\s*▴$/);
    const closed = await tray.boundingBox();
    expect(Math.abs(closed.y + closed.height - box.y), "closed: the same tray, one line high").toBeLessThan(2);
    await tray.locator(".crew-w").click();
    await expect(tray.locator(".crew-body"), "a click on its line opens it").toBeVisible();
    expect(await tray.locator(".crew-body").evaluate((el) => el.scrollTop), "never opened scrolled to the middle").toBe(0);
    await tray.locator(".crew-w").click();
    await expect(tray.locator(".crew-body")).toHaveCount(0);
    await tray.locator(".crew-w").click();
    // Esc goes to a box with text in it; an empty box (or none focused) gives it to the tray.
    const text = page.locator('textarea[data-text="s:lead"]');
    await text.fill("hello");
    await text.press("Escape");
    await expect(tray.locator(".crew-body")).toBeVisible();
    await text.fill("");
    await text.press("Escape");
    await expect(tray.locator(".crew-body")).toHaveCount(0);
    await tray.locator(".crew-w").click();
    await page.locator(".ap-chat").first().click({ position: { x: 20, y: 20 } });
    await page.keyboard.press("Escape");
    await expect(tray.locator(".crew-body")).toHaveCount(0);
  });

  test("open, the chat's newest message sits just above the tray; closed, at the chat's end", async ({ page }) => {
    const tray = page.locator(".crew-tray");
    await expect(tray.locator(".crew-body")).toBeVisible();
    const where = () => page.evaluate(() => {
      const chat = document.querySelector(".active-pane .ap-chat"), last = [...chat.children].filter((e) => e.offsetHeight).at(-1);
      return { last: last.getBoundingClientRect().bottom, tray: document.querySelector(".crew-tray").getBoundingClientRect().top, end: chat.scrollHeight - chat.scrollTop - chat.clientHeight < 2 };
    });
    const open = await where();
    expect(open.end, "the chat at its end").toBe(true);
    expect(open.last, "its newest message isn't under the tray").toBeLessThanOrEqual(open.tray);
    expect(open.tray - open.last, "just above it").toBeLessThan(32);
    await tray.locator(".crew-car").click();
    await expect(tray.locator(".crew-body")).toHaveCount(0);
    const closed = await where();
    expect(closed.end, "closed: still at its end").toBe(true);
    expect(closed.tray - closed.last, "its newest message just above the closed tray").toBeLessThan(32);
  });

  test("the tray opens by itself once when a member starts waiting on you; closed, it stays closed", async ({ cue, page }) => {
    await askMe(cue);
    const body = page.locator(".crew-tray .crew-body");
    await expect(body, "handoff and docs wait on you: open").toBeVisible();
    await page.locator(".crew-tray .crew-car").click();
    await cue.change((st) => { const lead = st.sessions.find((s) => s.session_id === "lead"); lead.team.find((t) => t.name === "rows").step = "Editing ui/a.js"; });
    await expect(page.locator(".crew-tray")).toBeVisible();
    await expect(body, "nothing new waits: stays closed").toHaveCount(0);
    await cue.change((st) => { const lead = st.sessions.find((s) => s.session_id === "lead"); lead.team.find((t) => t.name === "skill").state = "reviewed"; });
    await expect(body, "skill is ready: open again").toBeVisible();
  });

  test("rows agree with their buttons; done members fold into N done; a name opens its menu; no Set aside", async ({ cue, page }) => {
    const tray = page.locator(".crew-tray");
    await askMe(cue);
    const handoff = tray.locator(".mt-row", { hasText: "handoff" });
    await expect(handoff.locator(".mt-n")).toHaveText("handoff2 findings · fixes go back, round 1 of 1");
    await expect(handoff.locator("[data-mt-act]")).toHaveText(["Send fixes"]);
    await expect(handoff.locator(".mt-fx li")).toHaveCount(2);
    // docs's review says commit and lists only a note: a note needs no change, so Commit.
    const docs = tray.locator(".mt-row", { hasText: "docs" });
    await expect(docs.locator(".mt-n i")).toHaveText("1 note · commit it");
    await expect(docs.locator("[data-mt-act]")).toHaveText(["Commit"]);
    await expect(tray.locator(".mt-row, .mt-one", { hasText: "skill" }), "done: out of the way").toHaveCount(0);
    await expect(tray.locator(".mt-done-line")).toHaveText("1 done ›");
    await tray.locator(".mt-done-line").click();
    await expect(tray.locator(".mt-done .mt-one", { hasText: "skill" }).locator('[data-mt-act="diff"]')).toHaveText("Diff");
    await expect(tray.locator(".mt-done-line")).toHaveText("1 done ⌄");
    await expect(tray.locator("svg, [data-mt-act='aside']"), "no person icons, no Set aside").toHaveCount(0);
    await expect(tray, "no ⋯").not.toContainText("⋯");
    await handoff.locator('[data-mt-more="lead|id-handoff"]').click();
    const menu = page.locator(".crew-tray .mt-menu");
    await expect(menu.locator("button")).toHaveText(["Its steps", "Commit anyway", "Diff", "What it was asked"]);
    const name = await handoff.locator(".mt-name").boundingBox(), m = await menu.boundingBox();
    expect(Math.abs(m.x - name.x), "under the name").toBeLessThan(2);
    await menu.locator("[data-mt-asked]").click();
    await expect(handoff.locator(".mt-asked")).toContainText("Carry the team over a handoff");
    await cue.change((st) => {
      const lead = st.sessions.find((s) => s.session_id === "lead"), at = (n) => lead.team.find((t) => t.name === n);
      Object.assign(at("rows"), { state: "done", needs: "Its lead didn't say it committed it" });
    });
    const rows = tray.locator(".mt-row").filter({ has: page.locator(".mt-name", { hasText: /^rows$/ }) });
    await expect(tray.locator(".mt-grp").first()).toHaveText("Needs you · 1");
    await expect(rows.locator(".mt-n i")).toHaveText("Its lead didn't say it committed it");
    await expect(tray.locator(".crew-head")).toContainText("rows needs you");
    await expect(page.locator(".active-pane"), "no Set aside anywhere").not.toContainText("Set aside");
    await handoff.locator('[data-mt-act="fixes"]').click();
    expect((await cue.called("team_action", { action: "fixes" })).args).toMatchObject({ sessionId: "lead", agent: "id-handoff" });
    await docs.locator('[data-mt-act="commit"]').click();
    expect((await cue.called("team_action", { action: "commit" })).args).toMatchObject({ sessionId: "lead", agent: "id-docs" });
  });

  test("Cue's review request is Cue's bubble on the right, the review folded inside it, no card of its own", async ({ page }) => {
    await expect(page.locator(".cv-you:not(.cue-you)", { hasText: "Review your teammate" })).toHaveCount(0);
    await expect(page.locator(".cv-review, [data-crew-open]"), "no review cards").toHaveCount(0);
    await page.locator(".crew-tray .crew-car").click();
    const asked = page.locator(".cue-you", { hasText: "Asked for handoff's review" });
    await expect(asked).toHaveCount(1);
    await expect(asked.locator(".cv-meta")).toHaveText(/^Cue · /);
    await expect(asked.locator(".cv-voice-line")).toHaveCount(0);
    await asked.locator("[data-ask-open]").first().click();
    await expect(asked.locator("[data-ask-open]").first()).toHaveText("− What Cue sent");
    await expect(asked.locator(".cv-voice-line", { hasText: "Review your teammate handoff's work" })).toHaveCount(1);
    await expect(asked.locator('[data-ask-open$=":review"]')).toHaveText(/^\+ handoff reviewed: /);
    await expect(page.locator(".cv-agent", { hasText: "Recommendation" }), "the review isn't a message of its own").toHaveCount(0);
  });

  test("a crew member's Diff opens the lead's Diff tab on its files, and ✕ goes back to the whole diff", async ({ cue, page }) => {
    const tray = page.locator(".crew-tray");
    if (!(await tray.locator(".crew-body").count())) await tray.locator(".crew-car").click();
    await expect(page.locator('.mt-one [data-mt-act="stop"]').first()).toBeVisible();
    await expect(page.locator('[data-mt-act="diff"][data-mt-agent="id-hub"]'), "hub has no files Cue knows: no Diff").toHaveCount(0);
    await tray.locator(".mt-done-line").click();
    await page.locator('[data-mt-act="diff"][data-mt-agent="id-skill"]').click();
    expect((await cue.called("team_action", { action: "diff" })).args.agent).toBe("id-skill");
    await expect(page.locator(".stab.on")).toHaveText("Diff");
    await expect(page.locator(".dv-chip")).toHaveText(/^skill's 2 files/);
    await expect(page.locator(".dv-file .dv-path")).toHaveText(["skills/team/SKILL.md", "skills/team/review.md"]);
    await expect(page.locator(".dv-repo"), "one repo: no repo on each file").toHaveCount(0);
    await page.locator("[data-dv-all]").click();
    await expect(page.locator(".dv-chip")).toHaveCount(0);
    await expect(page.locator(".dv-seg")).toBeVisible();
    await expect(page.locator(".dv-file")).toHaveCount(0);
    // Chat, and the Diff tab goes: the lead has no changes of its own.
    await page.locator('.stab[data-stab=""]').click();
    await expect(page.locator('.stab[data-stab=":diff"]')).toHaveCount(0);
  });

  test("what Cue will send the lead once it's free waits on the member's row, never in the chat; Skip takes a review off the queue", async ({ cue, page }) => {
    await cue.change((st) => {
      const lead = st.sessions.find((s) => s.session_id === "lead");
      lead.team.find((t) => t.name === "skill").state = "queued";
      Object.assign(lead.team.find((t) => t.name === "docs"), { commit_wait_ms: Date.now() - 60000, files: [] });
      st.review_queue = { lead: [{ exec: "team:lead:id-skill", name: "skill" }] };
    });
    const tray = page.locator(".crew-tray");
    if (!(await tray.locator(".crew-body").count())) await tray.locator(".crew-car").click();
    await expect(page.locator(".cue-you", { hasText: /Will ask|asks its lead/ }), "nothing of it in the chat").toHaveCount(0);
    const skill = tray.locator(".mt-one", { hasText: "skill" });
    await expect(skill.locator(".mt-line")).toHaveText(/review waits for the lead$/);
    const docs = tray.locator(".mt-one", { hasText: "docs" });
    await expect(docs.locator(".mt-line")).toHaveText(/Cue asks its lead to commit it when it's free$/);
    await expect(docs.locator("button:not(.mt-name)"), "nothing for you to press").toHaveCount(0);
    await skill.locator('[data-act="unqueue-review"]').click();
    expect((await cue.called("unqueue_review")).args.exec).toBe("team:lead:id-skill");
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
