// The rest of the board: parking and resuming, stars, the project switcher, search, and
// Settings switches. Each checks the call the window made and what it shows once Cue's state comes back.
import { test, expect, pick } from "./cue.mjs";
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
    await pick(page, "agent-relay");
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
  // each draw (core/helpers.js bar). Its stretches are one masked layer a kind, and the one still going.
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
    await page.locator('[data-act="set-tab"][data-tab="insights"]').click();
    await page.locator('.sheet [data-set="tips.on"]').click();
    await cue.called("set_setting", { key: "tips.on", value: false });
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

// A waiting stretch wide enough for its stripes: striped, each one outlined at both its ends (core/helpers.js barLayerStyle's
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
// chat's place; ‹ Back to the board, Open chat › and Esc go back to the chat. The left is the lead's chat itself (every
// reply whole, its decisions in it at their time), Needs you over its box (one row, "N more ›"), the chat's box (the
// same draft); the list on the right: The lead, then the crew's rows in their own order, the finished ones folded.
test.describe("a hosted lead's crew on its own page", () => {
  const TEAM_ASK = "n1793984390004";
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("world-team");
    await openCard(page, TEAM_ASK);
    await page.locator('.crew-tray [data-cp-open="lead"]').click();
    await expect(page.locator('.crew-page[data-crew-page="lead"]')).toBeVisible();
  });
  const rowKeys = (page) => page.locator(".crew-page .cp-scroll .cp-row").evaluateAll((els) => els.map((e) => e.dataset.key.split("|id-")[1]));

  test("Open full opens it across the window: the lead's chat on the left, the list on the right; Back to the board returns to the chat, its draft carried over", async ({ page }) => {
    await expect(page.locator(".col-waiting, .col-sessions, .col.main"), "the page in the chat's and the columns' place").toHaveCount(0);
    await expect(page.locator(".crew-page .cp-left > .active-pane"), "its left the board's chat pane itself").toHaveCount(1);
    await expect(page.locator(".switch-view"), "the tabs stay").toBeVisible();
    const cp = await page.locator(".crew-page").boundingBox(), vw = page.viewportSize();
    expect(cp.width, "full width").toBeGreaterThan(vw.width - 80);
    // Left | list: the chat's own element on the left, as wide as the board's chat pane (checked once back on the
    // board, below), the list the rest of the width on the right.
    const left = await page.locator(".cp-left").boundingBox(), list = await page.locator(".cp-list").boundingBox();
    expect(left.x + left.width).toBeLessThanOrEqual(list.x + 1);
    expect(Math.round(left.width + list.width), "the list has the rest").toBe(Math.round(cp.width));
    await expect(page.locator(".cp-left .active-pane > .ap-chat[data-chat]"), "the chat's body, kept between draws as the chat's is").toHaveCount(1);
    // The list: The lead lit on top, then the members still in the crew, the set-aside one folded at the end.
    await expect(page.locator(".cp-list .cp-lead.on")).toContainText("The leadthe chat");
    await expect(page.locator(".crew-page .cp-scroll > .cp-cards > .cp-card"), "the members still in the crew").toHaveCount(5);
    await expect(page.locator(".cp-status .mt-grp")).toHaveText("The crew · 6");
    await expect(page.locator(".cp-status .cp-q"), "hub asks (the reviewed ones Cue handles itself)").toHaveText("1 needs you");
    await expect(page.locator(".cp-status [data-crew-menu]")).toHaveText("Handle it ▾");
    // The goal on one line; All of it › opens the four boxes in place.
    await expect(page.locator(".cp-goal .cp-gt")).toContainText("Done well when: Each part reviewed");
    await page.locator("[data-cp-goal]").click();
    await expect(page.locator(".cp-goal.open .cg-box")).toHaveCount(4);
    await page.locator("[data-cp-goal]").click();
    await expect(page.locator(".cp-goal .cg-box")).toHaveCount(0);
    // The chat's rows in time order, each line of the crew's log among them; Undo this only on what can be undone.
    const chat = page.locator(".cp-left .active-pane > .ap-chat");
    const [lines, undoable] = await page.evaluate(() => { const l = sessionOf("lead").crew_log || []; return [l.length, l.filter((d) => Object.hasOwn(CREW_UNDO, d.kind)).length]; });
    await expect(chat.locator(".cp-dec .cd-row.big")).toHaveCount(lines);
    await expect(chat.locator(".cd-row.big [data-act=crew-back]")).toHaveCount(undoable);
    await expect(chat.locator(".cd-row.big.step [data-act=crew-back]")).toHaveCount(0);
    const order = await chat.locator(":scope > *").evaluateAll((els) => els.map((e) => e.dataset.key || ""));
    const said = order.filter((k) => /^[ay]:\d+$/.test(k)).map((k) => +k.slice(2));
    expect(said.length, "the lead's replies and your messages").toBeGreaterThan(0);
    expect(said, "newest last").toEqual([...said].sort((a, b) => a - b));
    // Its box is the chat's: what's typed there is there when you go back.
    await page.locator('.cp-left textarea[data-text="s:lead"]').fill("carry on");
    await expect(page.locator("[data-cp-back]")).toHaveText("‹ Back to the board");
    await page.locator("[data-cp-back]").click();
    await expect(page.locator(".crew-page")).toHaveCount(0);
    await expect(page.locator(".col-waiting"), "the board's columns are back").toBeVisible();
    await expect(page.locator('.active-pane textarea[data-text="s:lead"]'), "the chat as it was, the draft in its box").toHaveValue("carry on");
    expect(Math.round((await page.locator(".active-pane").boundingBox()).width), "the page's chat as wide as the board's").toBe(Math.round(left.width));
  });

  test("Esc goes back to the chat, not while the box has text; Open chat goes back too", async ({ page }) => {
    const box = page.locator('.crew-page textarea[data-text="s:lead"]');
    await box.fill("@rows ");
    await box.press("Escape");
    await expect(page.locator(".crew-page"), "the box had text: Esc was the box's").toBeVisible();
    await box.fill("");
    await page.keyboard.press("Escape");
    await expect(page.locator(".crew-page")).toHaveCount(0);
    await expect(page.locator(".active-pane .crew-tray")).toBeVisible();
    await page.locator('.crew-tray [data-cp-open="lead"]').click();
    await page.locator("[data-cp-chat]").click();
    await expect(page.locator(".crew-page")).toHaveCount(0);
    await expect(page.locator(".active-pane")).toBeVisible();
  });

  test("the left is the chat: the reply the lead stopped on when it waits on you, and a message queued in it", async ({ cue, page }) => {
    const chat = page.locator(".cp-left .active-pane > .ap-chat");
    await cue.change((st) => {
      const lead = st.sessions.find((s) => s.session_id === "lead");
      lead.state = "waiting";
      st.items = st.items.filter((i) => i.session_id !== "lead");
      st.items.push({ id: "w-lead", kind: "waiting", harness: "claude", session_id: "lead", cwd: lead.cwd, project: lead.project, message: "All five are in: docs is committed, the rest wait on review. Your turn.", created_ms: lead.thread.at(-1).at_ms + 1000 });
    });
    await expect(chat.locator(".cv-agent").last(), "its last reply, from its waiting card").toContainText("All five are in: docs is committed");
    await expect(page.locator(".cp-lead .cp-lr")).toHaveText("your turn");
    await cue.change((st) => {
      const lead = st.sessions.find((s) => s.session_id === "lead");
      lead.state = "working";
      lead.thread.push({ role: "you", text: "go on with the rest", at_ms: lead.thread.at(-1).at_ms + 1500, images: [] });
      lead.queued = { role: "you", text: "and keep spike out of it", at_ms: lead.thread.at(-1).at_ms + 500, images: [] };
    });
    await expect(chat.locator(".cv-you.queued").last(), "what you queued, newest last").toContainText("and keep spike out of it");
    // #422 (A): not with it yet, a dashed outline on the panel; the filled bubble once delivered; the line under it says which.
    const look = (l) => l.locator(".cv-you-text").first().evaluate((el) => { const c = getComputedStyle(el); return { bg: c.backgroundColor, border: c.borderTopStyle }; });
    const queued = await look(chat.locator(".cv-you.queued").last()), sent = await look(chat.locator(".cv-you:not(.queued):not(.cue-you)").last());
    expect(queued.border, "queued is dashed").toBe("dashed");
    expect(sent.border, "delivered is filled, no outline").toBe("none");
    expect(queued.bg, "queued is on the panel, not filled").not.toBe(sent.bg);
    await expect(chat.locator(".cv-you.queued .mg-f").last()).toContainText("Queued for");
    await expect(page.locator(".cp-lead .cp-lr")).toHaveText(/^working · /);
  });

  test("the lead's judgment call sits in the chat at its time, with Undo this; a step there has none", async ({ cue, page }) => {
    const chat = page.locator(".cp-left .active-pane > .ap-chat");
    await cue.change((st) => {
      const lead = st.sessions.find((s) => s.session_id === "lead"), t = lead.thread;
      lead.crew_log = [
        { id: "d1-0", at_ms: t[1].at_ms + 1, who: "handoff", call: "call-handoff", kind: "fixes", what: "Reviewed: 1 should-fix. Sent the fix back", why: "the list is cut at 8" },
        { id: "d2-0", at_ms: t[1].at_ms + 2, who: "lead", kind: "decided", what: "Kept the list cut at 8", why: "the board has no room for more" },
      ];
    });
    const step = chat.locator('.cp-dec[data-key="cd:d1-0"]'), call = chat.locator('.cp-dec[data-key="cd:d2-0"]');
    await expect(step.locator(".cd-what")).toHaveText("Reviewed: 1 should-fix. Sent the fix back");
    await expect(step.locator(".cd-row.step"), "fixes sent: a quiet step").toHaveCount(1);
    await expect(step.locator("[data-act=crew-back]"), "nothing to undo").toHaveCount(0);
    await expect(call.locator(".cd-kind")).toHaveText("Decided");
    await expect(call.locator(".cd-what")).toHaveText("Kept the list cut at 8");
    await expect(call.locator("[data-act=crew-back]")).toHaveText("Undo this");
    const keys = await chat.locator(":scope > *").evaluateAll((els) => els.map((e) => e.dataset.key || ""));
    const at = await page.evaluate(() => sessionOf("lead").thread.map((e) => e.at_ms));
    // Between Cue's review request for handoff (its review folded into it) and the next one, for docs.
    const i = keys.indexOf("cd:d1-0"), before = keys.indexOf(`y:${at[0]}`), after = keys.indexOf(`y:${at[2]}`);
    expect(before, "the row before it is drawn").toBeGreaterThanOrEqual(0);
    expect(before, "after what came before it").toBeLessThan(i);
    expect(after, "before what came after it").toBeGreaterThan(keys.indexOf("cd:d2-0"));
    expect(keys.indexOf("cd:d2-0"), "in time order").toBe(i + 1);
  });

  test("replies fold as the chat folds them: the 3 newest whole however long, no Show all on them; an older long one folds with the chat's own button", async ({ cue, page }) => {
    const rowsMd = Array.from({ length: 30 }, (_, i) => `| part ${i} | done |`).join("\n");
    const long = (w) => Array.from({ length: 12 }, (_, i) => `${w} paragraph ${i}, long enough that the chat would fold it.`).join("\n\n");
    await cue.change((st, [rowsMd, texts]) => {
      const lead = st.sessions.find((s) => s.session_id === "lead"), t = lead.thread.at(-1).at_ms;
      texts.forEach((text, n) => lead.thread.push({ role: "agent", text, at_ms: t + 100 + n, images: [] }));
      lead.thread.push({ role: "agent", text: `Where the crew is:\n\n| part | state |\n|---|---|\n${rowsMd}\n\nThat's all.`, at_ms: t + 500, images: [] });
    }, [rowsMd, [long("Oldest"), long("Second"), long("Third")]]);
    const chat = page.locator(".cp-left .active-pane > .ap-chat"), reply = (w) => chat.locator(".cv-agent").filter({ hasText: w });
    await expect(reply("Where the crew is")).toBeVisible();
    // The chat sits at its end, as it was: the newest reply's end in view.
    await expect.poll(() => page.evaluate(() => { const c = document.querySelector(".cp-left .active-pane > .ap-chat"); return c.scrollHeight - c.scrollTop - c.clientHeight; })).toBeLessThan(30);
    // The three newest, every one long: whole, nothing to Show all or Fold.
    for (const w of ["Second paragraph 0", "Third paragraph 0", "Where the crew is"]) {
      await expect(reply(w), w).not.toHaveClass(/folded/);
      await expect(reply(w).locator(".cv-more, .cp-more"), `${w}: no Show all`).toHaveCount(0);
    }
    await expect(reply("Where the crew is").locator(".cv-text tbody tr"), "the table whole").toHaveCount(30);
    await expect(reply("Where the crew is").locator(".cv-text")).toContainText("That's all.");
    await expect(reply("Second paragraph 0").locator(".cv-text")).toContainText("Second paragraph 11");
    // The fourth newest, long: folded, as in the chat, by the chat's own button; it opens in place.
    const oldest = reply("Oldest paragraph 0");
    await expect(oldest).toHaveClass(/folded/);
    await expect(oldest.locator(".cv-more")).toHaveText("Show all");
    await expect(chat.locator(".cp-more"), "no cut of the page's own").toHaveCount(0);
    await oldest.locator(".cv-more").click();
    await expect(oldest).not.toHaveClass(/folded/);
    await expect(oldest.locator(".cv-more")).toHaveText("Fold");
  });

  test("with the board's columns folded, or full screen, the chat keeps the width it has beside them and the list its room", async ({ page }) => {
    const widths = () => page.evaluate(() => ({ page: document.querySelector(".crew-page").getBoundingClientRect().width, left: document.querySelector(".cp-left").getBoundingClientRect().width, list: document.querySelector(".cp-list").getBoundingClientRect().width }));
    const open = await widths();
    // Both folded by you.
    await page.evaluate(() => { collapsed.add("waiting"); collapsed.add("sessions"); renderMain(); });
    const folded = await widths();
    expect(Math.round(folded.left), "both folded: the chat as wide as beside both").toBe(Math.round(open.left));
    expect(folded.list, "the list keeps its room").toBeGreaterThan(500);
    // Full screen: every column folded until you open one there.
    await page.evaluate(() => { collapsed.clear(); focus = true; focusShown = new Set(); renderMain(); });
    const full = await widths();
    expect(Math.round(full.left), "full screen: the same").toBe(Math.round((full.page - 28) * 1.9 / 3.75));
    expect(full.list).toBeGreaterThan(500);
    await page.evaluate(() => { focus = false; renderMain(); });
  });

  test("the head: the name, Cue Crew and the crew's state in a word; the chat pane's own title line, btw, waiting line and box under it", async ({ cue, page }) => {
    const head = page.locator(".crew-page .cp-head"), pane = page.locator(".crew-page .cp-left > .active-pane");
    await expect(head.locator(".cp-name")).toHaveText("cue");
    await expect(head.locator(".cp-kind")).toHaveText("Cue Crew");
    await expect(head.locator(".cp-pill"), "hub asks you").toHaveText("needs you");
    await expect(head.locator(".cp-pill")).toHaveClass(/\bneed\b/);
    // What the pane's title line has isn't repeated in the head: its ⋯, btw, Stop and folder are the pane's.
    await expect(head.locator("[data-act=btw], [data-act=interrupt], .cp-proj")).toHaveCount(0);
    await expect(pane.locator(".ap-head [data-act=btw]")).toHaveText("btw");
    await expect(pane.locator(".ap-chat .cv-status"), "the chat's waiting line").toContainText("Waiting on you");
    await expect(pane.locator(".crew-tray"), "the crew is in the list, not the tray").toHaveCount(0);
    // btw opens its panel in the pane, as on the board.
    await pane.locator(".ap-head [data-act=btw]").click();
    await expect(pane.locator(".btw-pop")).toBeVisible();
    await expect(pane.locator(".btw-head"), "it says whom it asks").toContainText("BY THE WAY · ASKS THE LEAD, NOT PART OF ITS CHAT");
    await expect(pane.locator(".btw-ask textarea")).toHaveAttribute("placeholder", "Ask the lead without interrupting it…  ↵");
    await page.locator("[data-act=btw-close]").click();
    await expect(page.locator(".btw-pop")).toHaveCount(0);
    // Nobody asks and the lead works: working; Stop is the pane's.
    await cue.change((st) => {
      st.items = st.items.filter((i) => i.session_id !== "lead");
      const lead = st.sessions.find((s) => s.session_id === "lead");
      lead.state = "working";
      for (const t of lead.team) t.needs = "";
    });
    await expect(head.locator(".cp-pill")).toHaveText("working");
    await expect(pane.locator(".ap-head [data-act=interrupt]")).toHaveText("Stop");
  });

  test("its left is the lead's pane whatever the board has open: nothing, or a Quiet session", async ({ page }) => {
    const left = page.locator(".crew-page .cp-left > .active-pane");
    const isLead = async (why) => {
      await expect(left.locator('textarea[data-text="s:lead"]'), why).toHaveCount(1);
      await expect(left, why).not.toHaveClass(/\bempty\b/);
    };
    await isLead("opened from the lead's card");
    // Nothing open on the board.
    await page.evaluate(() => { active = null; renderMain(); });
    await isLead("nothing active");
    // A Quiet session open (one not in the world: its pane would give up and fall back to what's open).
    await page.evaluate(() => { quietOpen = "q-elsewhere"; renderMain(); });
    await isLead("Quiet open");
    await page.evaluate(() => { quietOpen = null; renderMain(); });
  });

  test("the page fits the window and never scrolls as a whole; the chat and the list scroll inside themselves", async ({ page }) => {
    for (const vp of [{ width: 1280, height: 800 }, { width: 1280, height: 1000 }, { width: 900, height: 600 }]) {
      await page.setViewportSize(vp);
      if (!(await page.locator(".cp-done.open").count())) await page.locator(".cp-done .mt-done-line").click();
      const z = await page.evaluate(() => {
        const p = document.querySelector(".crew-page"), r = (s) => document.querySelector(s).getBoundingClientRect();
        return { over: p.scrollHeight - p.clientHeight, bottom: r(".cp-left .foot").bottom - r(".crew-page").bottom, page: r(".crew-page").width, left: r(".cp-left").width, cards: [...document.querySelectorAll(".cp-cards > .cp-card")].map((c) => [Math.round(c.getBoundingClientRect().left), Math.round(c.getBoundingClientRect().width)]), list: r(".cp-scroll").width };
      });
      expect(z.over, `${vp.width}×${vp.height}: the page itself doesn't scroll`).toBeLessThanOrEqual(1);
      expect(z.bottom, `${vp.width}×${vp.height}: the box in view`).toBeLessThanOrEqual(0);
      // The board's chat pane: Active's 1.9 of its 3.75 shares, after the columns' two 14px gaps (board/columns.js colLayout).
      expect(Math.round(z.left), `${vp.width}×${vp.height}: the chat as wide as on the board`).toBe(Math.round((z.page - 28) * 1.9 / 3.75));
      expect(new Set(z.cards.map(([x]) => x)).size, `${vp.width}×${vp.height}: one card to a line`).toBe(1);
      expect(z.cards[0][1], `${vp.width}×${vp.height}: as wide as the list`).toBe(Math.round(z.list));
    }
  });

  test("the list keeps the crew's order whatever their state; each row two calm lines; a click shows that teammate on the left and moves nothing; the finished ones fold at its end", async ({ cue, page }) => {
    // hub asks you, rows works, skill done, handoff and docs reviewed: the crew's own order.
    await expect.poll(() => rowKeys(page)).toEqual(["hub", "rows", "skill", "handoff", "docs"]);
    // The tab stays Board: the page opened from the board shows within it.
    await expect(page.locator(".switch-view button.on")).toHaveText("Board");
    // A row: its name, its stage and meter, how long; then one sentence, red when it needs you.
    const card = (n) => page.locator(`.cp-card[data-key="cpc:lead|id-${n}"]`);
    const hub = page.locator('.cp-row[data-key="cp:lead|id-hub"]'), rows = page.locator('.cp-row[data-key="cp:lead|id-rows"]');
    await expect(card("hub")).toHaveClass(/\bq\b/);
    await expect(hub.locator(".cp-ml")).toHaveText("needs you");
    await expect(hub.locator(".cp-nm")).toHaveText("hub");
    await expect(hub.locator(".cp-m b.need")).toHaveCount(1);
    await expect(hub.locator(".cp-now.q")).toHaveText("Wants to run cd src-tauri && cargo test --features ext");
    await expect(rows.locator(".cp-now")).toHaveText("Running node tests/team.test.js");
    await expect(rows.locator(".cp-ml"), "a test run is checking").toHaveText("checking");
    await expect(rows.locator(".cp-tm")).toHaveText(/^(<1m|\d+m)$/);
    await expect(card("handoff").locator(".cp-now")).toHaveText("1 should-fix, 1 note · Cue sends it back");
    await expect(card("skill").locator(".cp-now")).toHaveText("Team skill written, 12 tests pass");
    await expect(page.locator(".cp-scroll .cp-row > *"), "two lines, nothing more").toHaveCount(10);
    expect(await page.locator(".cp-scroll .cp-row button").count(), "no buttons in a row").toBe(0);
    // Beside a working one's row, outside it, the drawer's Stop: it stops it, from here.
    const stop = card("rows").locator('.cp-act [data-mt-act="stop"]');
    await expect(stop).toHaveText("Stop");
    await stop.click();
    expect((await cue.called("team_action", { sessionId: "lead", action: "stop" })).args.agent).toBe("id-rows");
    expect(await page.locator(".crew-page .cp-list [title]").count(), "no hover text on the list").toBe(0);
    // docs goes back to work, handoff needs you: nothing moves.
    await cue.change((st) => {
      const team = st.sessions.find((s) => s.session_id === "lead").team;
      Object.assign(team.find((t) => t.name === "docs"), { state: "working", step: "Editing docs" });
      team.find((t) => t.name === "handoff").needs = "commit";
    });
    await expect(card("handoff")).toHaveClass(/\bq\b/);
    expect(await rowKeys(page)).toEqual(["hub", "rows", "skill", "handoff", "docs"]);
    // A click on a row: its row lit, the left shows it; nothing moves; Esc goes back to the chat, not the page.
    const skill = page.locator('.cp-row[data-key="cp:lead|id-skill"]');
    await skill.click();
    await expect(skill).toHaveAttribute("aria-pressed", "true");
    await expect(card("skill")).toHaveClass(/\bon\b/);
    await expect(page.locator(".cp-lead"), "The lead isn't lit").not.toHaveClass(/\bon\b/);
    await expect(page.locator(".cp-left .cpm .cpv-name")).toHaveText("skill");
    expect(await rowKeys(page)).toEqual(["hub", "rows", "skill", "handoff", "docs"]);
    await page.keyboard.press("Escape");
    await expect(page.locator(".cp-left .cpm")).toHaveCount(0);
    await expect(page.locator(".cp-lead")).toHaveClass(/\bon\b/);
    await expect(page.locator(".crew-page"), "Esc went back to the chat first").toBeVisible();
    // The set-aside one: folded at the end; unfolded under the fold line, dimmed, the others where they were.
    const done = page.locator(".crew-page .cp-done .mt-done-line");
    await expect(done).toHaveText("1 done ›");
    await expect(page.locator(".crew-page .cp-card.past")).toHaveCount(0);
    // Where docs's row sits in the list (not on screen: the list scrolls to show what unfolds).
    const at = () => page.locator('.cp-row[data-key="cp:lead|id-docs"]').evaluate((el) => el.getBoundingClientRect().top - el.closest(".cp-scroll").getBoundingClientRect().top + el.closest(".cp-scroll").scrollTop);
    const before = await at();
    await done.click();
    await expect(done).toHaveText("1 done ⌄");
    await expect(page.locator(".crew-page .cp-done .cp-card.past")).toHaveCount(1);
    expect(await at(), "nothing above it moved").toBe(before);
    await expect(page.locator(".crew-page .cp-done .cp-card.past"), "what unfolded is scrolled into view").toBeInViewport();
    expect(await rowKeys(page)).toEqual(["hub", "rows", "skill", "handoff", "docs", "spike"]);
  });

  // Needs you (ui/team_bar.js): one bar over the box, one ask at a time, the same height whatever the count.
  const addAsks = (cue) => cue.change((st) => {
    const hub = st.items.find((i) => i.id === "n1793984390004");
    st.items.push({ ...hub, id: "own1", agent_id: "", agent_name: "", agent_pid: 0, tool_input: { command: "git push origin main" }, one_way: true, created_ms: hub.created_ms + 1 });
    st.items.push({ ...hub, id: "rows1", agent_id: "id-rows", agent_name: "rows", tool_input: { command: "npm run migrate" }, created_ms: hub.created_ms - 5 });
  });
  test("Needs you is one bar of one height: one ask, its count and steppers with more; nothing at all when nothing waits", async ({ cue, page }) => {
    const bar = page.locator(".cp-left .cp-bar");
    await expect(bar.locator(".cpb-who")).toHaveText("hub");
    await expect(bar.locator(".cpb-what")).toHaveText("wants to run");
    await expect(bar.locator(".cpb-cmd")).toHaveText("cd src-tauri && cargo test --features ext");
    await expect(bar.locator(".cpb-btns button")).toHaveText(["↪", "Deny", "Allow"]);
    await expect(bar.locator(".cpb-n, [data-cp-step], [data-cp-all].cpb-all"), "one: no count, no steppers").toHaveCount(0);
    const one = (await bar.boundingBox()).height;
    expect(Math.round(one)).toBe(68);
    // Over the box, at the bottom of the left.
    const [b, f] = await Promise.all([bar.boundingBox(), page.locator(".cp-left .active-pane > .foot").boundingBox()]);
    expect(b.y + b.height).toBeLessThanOrEqual(f.y + 1);
    // Two more: the lead's own (one-way) first, then the oldest; the one shown stays shown, only the count rises.
    await addAsks(cue);
    await expect(bar.locator(".cpb-n"), "hub, last of the three now").toHaveText("3 of 3");
    await expect(bar.locator(".cpb-who"), "a new ask never swaps the one shown").toHaveText("hub");
    expect(Math.round((await bar.boundingBox()).height), "the same height").toBe(68);
    await expect(page.locator(".cp-status .cp-q"), "the list counts the crew's asks").toHaveText("2 need you");
    const order = await page.evaluate(() => crewAsks("lead").map((x) => x.key));
    expect(order).toEqual(["a:own1", "a:rows1", "a:n1793984390004"]);
    await page.waitForTimeout(650);
    await bar.locator('[data-cp-step="1"]').click();
    await expect(bar.locator(".cpb-n"), "round the end").toHaveText("1 of 3");
    await expect(bar.locator(".cpb-who")).toHaveText("The lead");
    await expect(bar.locator(".cpb-tag")).toHaveText("one-way");
    await expect(bar.locator(".cpb-ow"), "one-way: a click, never a key").toHaveText("one-way: answered with a click, never a key");
    await expect(bar.locator("[data-bar-key]"), "no key on a one-way ask").toHaveCount(0);
    // The lead asks: the box holds, in place, its draft kept.
    await expect(page.locator(".cp-left .foot.cp-held textarea")).toHaveAttribute("placeholder", "The lead asks too (1 of 3): answer it first. Your message waits here…");
    await expect(page.locator(".cp-left .foot.cp-held .btn.send")).toHaveText("Answer first");
    // Answered and handled: the bar goes, the chat runs down to the box.
    await cue.change((st) => {
      st.items = st.items.filter((i) => i.session_id !== "lead");
      const team = st.sessions.find((s) => s.session_id === "lead").team;
      for (const t of team) if (t.state === "reviewed") t.state = "committed";
    });
    await expect(bar).toHaveCount(0);
    await expect(page.locator(".cp-left .foot.cp-held")).toHaveCount(0);
  });

  test("the bar's buttons are dead for a moment after the ask changes; an answer puts the cursor in the box", async ({ cue, page }) => {
    await addAsks(cue);
    const bar = page.locator(".cp-left .cp-bar");
    await page.waitForTimeout(650);
    await bar.locator('[data-cp-step="-1"]').click();
    await expect(bar.locator(".cpb-who")).toHaveText("rows");
    await expect(bar).toHaveClass(/\bdead\b/);
    await bar.locator('[data-act="allow"]').click({ force: true });
    expect(await cue.calls("respond"), "a click in the first moment goes nowhere").toHaveLength(0);
    await expect(bar).not.toHaveClass(/\bdead\b/);
    await bar.locator('[data-act="allow"]').click();
    expect((await cue.called("respond", { id: "rows1" })).args.decision.behavior).toBe("allow");
    await expect.poll(() => page.evaluate(() => document.activeElement?.dataset?.text)).toBe("s:lead");
  });

  test("All N › puts every ask on the left, each whole; Esc and ‹ The lead's chat go back", async ({ cue, page }) => {
    await addAsks(cue);
    await page.locator(".cp-bar .cpb-all").click();
    const left = page.locator(".cp-left .active-pane.cp-alt");
    await expect(left.locator(".cpv-head .cpv-name")).toHaveText("Needs you");
    await expect(left.locator(".cpn-row")).toHaveCount(3);
    await expect(left.locator(".cpn-row").first()).toContainText("git push origin main");
    await expect(page.locator(".cp-bar"), "the page is the bar while it's up").toHaveCount(0);
    await expect(page.locator('.cp-left textarea[data-text="s:lead"]'), "the box stays").toHaveCount(1);
    await page.keyboard.press("Escape");
    await expect(left).toHaveCount(0);
    await expect(page.locator(".cp-bar")).toBeVisible();
    await page.locator(".cp-bar .cpb-all").click();
    await page.locator(".cpv-back").click();
    await expect(page.locator(".cp-left .active-pane > .ap-chat")).toHaveCount(1);
  });

  test("↪ puts a chip in the box: its words go as that answer; × takes it off", async ({ cue, page }) => {
    const bar = page.locator(".cp-left .cp-bar");
    await bar.locator("[data-cp-chip]").click();
    await expect(page.locator(".cp-chip")).toContainText("↪ instead, to hub");
    const box = page.locator('.cp-left textarea[data-text="s:lead"]');
    await expect(box).toBeFocused();
    await box.fill("run only the team tests");
    await box.press("Enter");
    const r = await cue.called("respond", { id: "n1793984390004" });
    expect(r.args.decision).toEqual({ behavior: "deny", message: "run only the team tests" });
    await expect(page.locator(".cp-chip")).toHaveCount(0);
    expect(await cue.calls("send_to_session"), "nothing sent to the lead").toHaveLength(0);
  });

  // A teammate on the left (ui/team_mate.js): its head line, Asked, its files as chips over the reading area; its steps
  // and review under its card in the list; the left takes all but a narrow list.
  test("a picked teammate on the left: its head line, what it was asked, file chips, a numbered diff and three tabs; steps and review under its card; Talk, the bar's ask, The lead's dot", async ({ cue, page }) => {
    // btw open on the chat: picking a teammate closes it with the pane's head.
    await page.locator(".cp-left .ap-head [data-act=btw]").click();
    await expect(page.locator(".btw-pop")).toBeVisible();
    await page.locator('.cp-row[data-cp-mate="lead|id-handoff"]').click();
    await expect(page.locator(".btw-pop")).toHaveCount(0);
    expect(await page.evaluate(() => btwFor), "closed, not left open out of sight").toBe(null);
    const v = page.locator(".cp-left .active-pane.cp-alt .cpm");
    await expect(v.locator(".cpv-back")).toHaveText("‹ The lead's chat");
    await expect(v.locator(".cpv-name")).toHaveText("handoff");
    await expect(v.locator(".cpv-head .cp-pill")).toHaveText("reviewed");
    await expect(v.locator(".cpm-now")).toHaveText("Reviewed: 1 should-fix, 1 note");
    await expect(v.locator(".cpm-at")).toHaveText("Carry the team over a handoff. Don't commit; finish with a summary.");
    await expect(v.locator(".cpm-size")).toHaveText(/^2 files · .*, since \d\d:\d\d$/);
    // The left takes all but the list's 330.
    expect(await page.locator(".crew-page .cp-list").evaluate((el) => Math.round(el.getBoundingClientRect().width))).toBe(330);
    // No side column: its steps and review unfold under its card in the list, and only its.
    await expect(v.locator(".cpm-st, .cpm-fl"), "not on the left").toHaveCount(0);
    const exp = page.locator('.cp-card[data-key="cpc:lead|id-handoff"] .cp-exp');
    await expect(page.locator(".cp-exp")).toHaveCount(1);
    await expect(exp.locator(".cpm-pl")).toHaveText(["Steps", "Review found"]);
    await expect(exp.locator(".cpm-st").filter({ hasText: "Started" }), "no feed of its own: what Cue keeps, by time").toHaveCount(1);
    await expect(exp.locator(".cpm-fl")).toHaveCount(2);
    await expect(exp.locator(".cpm-fl .cp-f.fix")).toHaveText("should-fix");
    // Its files as chips with their numbers (team_diff_stat), the first lit and its diff drawn (team_diff), each line
    // numbered; another file picked.
    await expect(v.locator(".cpm-chip .cpm-fp")).toHaveText(["handoff.rs", "core.js"]);
    await expect(v.locator(".cpm-chip .cpm-n")).toHaveText(["+2", "+2"]);
    await expect(v.locator(".cpm-chip.on .cpm-fp")).toHaveText("handoff.rs");
    await expect(v.locator(".cpm-diff .add")).toHaveText(["+# handoff.rs", "+handoff wrote this"]);
    const nums = await v.locator(".cpm-diff .ln").evaluateAll((els) => els.map((el) => [el.dataset.o, el.dataset.n]));
    expect(nums.length, "every line under a hunk's head numbered").toBeGreaterThan(0);
    expect(nums.every(([o, n]) => o || n)).toBe(true);
    await expect(v.locator(".cpm-lit")).toHaveText("src-tauri/src/hub/handoff.rs");
    await expect(v.getByText("Whole diff in the Diff view"), "no way out to the Diff view").toHaveCount(0);
    // A chip clicked: its whole diff over the page, as the Inbox opens one (diff_overlay.js), its files listed beside
    // it; ↑ the one before; Esc and the teammate is where it was, that chip lit.
    await v.locator('.cpm-chip[data-cp-file="ui/core.js"]').click();
    const ov = page.locator(".dov");
    await expect(ov.locator(".dov-title")).toHaveText("handoff");
    await expect(ov.locator(".dov-f .dov-p")).toHaveText(["src-tauri/src/hub/handoff.rs", "ui/core.js"]);
    await expect(ov.locator(".dov-f.on .dov-p")).toHaveText("ui/core.js");
    await expect(ov.locator(".dov-lines .a").first()).toHaveText("+# core.js");
    await page.keyboard.press("ArrowUp");
    await expect(ov.locator(".dov-lines .a")).toHaveText(["+# handoff.rs", "+handoff wrote this"]);
    await page.keyboard.press("Escape");
    await expect(page.locator(".dov")).toHaveCount(0);
    await expect(v.locator(".cpm-diff .add").first()).toHaveText("+# core.js");
    await expect(v.locator(".cpm-chip.on .cpm-fp")).toHaveText("core.js");
    await expect(v.locator(".cpm-tabs button:visible")).toHaveText(["Diff so far", "Full review", "Full task"]);
    await v.locator('[data-cp-tab="task"]').click();
    await expect(v.locator(".cpm-body")).toContainText("Carry the team over a handoff.");
    await v.locator('[data-cp-tab="review"]').click();
    await expect(v.locator(".cpm-body li")).toHaveCount(2);
    await expect(page.locator('.cp-left textarea[data-text="s:lead"]'), "the box stays").toHaveCount(1);
    await expect(page.locator(".cp-alt .quick"), "the chat's quick phrases aren't drawn under it").toHaveCount(0);
    // Talk: "@name " in the box.
    await v.locator("[data-cp-talk]").click();
    await expect(page.locator('.cp-left textarea[data-text="s:lead"]')).toHaveValue("@handoff ");
    // The lead's row, off the chat: its last words, and a dot once it says something new.
    const lead = page.locator(".cp-lead");
    await expect(lead).not.toHaveClass(/\bon\b/);
    await expect(lead.locator(".cp-said")).toHaveCount(0);
    await cue.change((st) => { const l = st.sessions.find((s) => s.session_id === "lead"); l.thread.push({ role: "agent", text: "handoff's fix is in.", at_ms: Date.now() + 1000, images: [] }); });
    await expect(lead.locator(".cp-ld")).toHaveText("“handoff's fix is in.”");
    await expect(lead.locator(".cp-said")).toHaveCount(1);
    // A teammate that asks: the bar shows its ask when it's picked.
    await page.locator('.cp-row[data-cp-mate="lead|id-hub"]').click();
    await expect(page.locator(".cpm .cpv-name")).toHaveText("hub");
    await expect(page.locator('.cp-card[data-key="cpc:lead|id-hub"] .cp-exp .cpm-st.ask')).toContainText("Wants to run cd src-tauri");
    await expect(page.locator(".cp-bar .cpb-who")).toHaveText("hub");
    // The lead's row: back to the chat.
    await lead.click();
    await expect(page.locator(".cp-left .cpm")).toHaveCount(0);
    await expect(lead).toHaveClass(/\bon\b/);
    await expect(page.locator(".cp-exp"), "nothing unfolded under a card").toHaveCount(0);
    expect(await page.locator(".crew-page .cp-list").evaluate((el) => Math.round(el.getBoundingClientRect().width)), "the halves back").toBeGreaterThan(330);
  });

  // The add-on's slots on the page (core/addon.js addOn.crewPage*): its page shares the one left with a teammate.
  test("the add-on's page on the left: in the chat's place under the pane's head; The lead's row, a teammate or Esc closes it; a record opens with no box", async ({ page }) => {
    await page.evaluate(() => {
      window.__left = { key: "loop:1", html: `<div class="their-page">Their page</div>` };
      window.__closed = 0; window.__esc = 0;
      addOn.crewPageLeft = () => window.__left;
      addOn.crewPageLeftClose = () => { const was = !!window.__left; window.__left = null; window.__closed++; return was; };
      addOn.crewPageEsc = () => { window.__esc++; return false; };
      addOn.crewPageGoal = () => `<div class="their-goal">Their goal</div>`;
      renderMain();
    });
    const pane = page.locator(".cp-left > .active-pane");
    await expect(pane.locator(".cp-addon-left .their-page")).toHaveText("Their page");
    await expect(pane.locator(".ap-head"), "the pane's head stays").toHaveCount(1);
    await expect(pane.locator(".cp-bar"), "Needs you stays").toHaveCount(1);
    await expect(pane.locator('textarea[data-text="s:lead"]'), "the box stays").toHaveCount(1);
    await expect(page.locator(".cp-lead")).not.toHaveClass(/\bon\b/);
    await expect(page.locator(".cp-top .their-goal"), "its goal row in place of the page's").toHaveCount(1);
    await expect(page.locator(".cp-goal")).toHaveCount(0);
    // A teammate picked: their page gives way.
    await page.locator('.cp-row[data-cp-mate="lead|id-rows"]').click();
    await expect(page.locator(".cp-left .cpm .cpv-name")).toHaveText("rows");
    expect(await page.evaluate(() => window.__closed)).toBe(1);
    // Theirs again; The lead's row goes back to the chat.
    await page.evaluate(() => { window.__left = { key: "loop:1", html: `<div class="their-page">Their page</div>` }; renderMain(); });
    await expect(page.locator(".cp-left .cpm"), "theirs is what the left shows").toHaveCount(0);
    await page.locator(".cp-lead").click();
    await expect(page.locator(".cp-left .their-page")).toHaveCount(0);
    await expect(page.locator(".cp-lead")).toHaveClass(/\bon\b/);
    // Esc asks theirs first (it says no), then on with the page's own order.
    await page.locator(".cp-left .ap-chat").first().click({ position: { x: 20, y: 20 } });
    await page.keyboard.press("Escape");
    expect(await page.evaluate(() => window.__esc)).toBeGreaterThan(0);
    // A record: no session, no box, no Needs you; its page on the left, its crew by name in the list.
    await page.evaluate(() => {
      addOn.crewPageRecord = (key) => key === "run:9" ? { mates: `<div class="their-mates">api · web</div>` } : null;
      window.__left = { key: "run:9", html: `<div class="their-record">Run 9</div>` };
      addOn.openCrewPage("run:9", {});
    });
    await expect(page.locator(".crew-page.cp-record .their-record")).toHaveText("Run 9");
    await expect(page.locator(".crew-page.cp-record .their-mates")).toHaveText("api · web");
    await expect(page.locator(".crew-page.cp-record textarea, .crew-page.cp-record .cp-bar, .crew-page.cp-record [data-cp-chat]")).toHaveCount(0);
    await page.evaluate(() => { for (const k of ["crewPageLeft", "crewPageLeftClose", "crewPageEsc", "crewPageGoal", "crewPageRecord"]) delete addOn[k]; });
  });

  // A big crew (SPEC §6): rows get shorter to fit; a chip for an ask out of view; ↑ / ↓ walk the list.
  const bigCrew = (cue, count) => cue.change((st, count) => {
    const lead = st.sessions.find((s) => s.session_id === "lead"), base = lead.team[1];
    lead.team = Array.from({ length: count }, (_, i) => ({ ...base, agent_id: `id-m${i}`, call: `call-m${i}`, name: `m${i}`, also: [], state: "working", step: "Editing handlers.ts" }));
    const hub = st.items.find((i) => i.id === "n1793984390004");
    hub.agent_id = `id-m${count - 1}`; hub.agent_name = `m${count - 1}`;
  }, count);
  test("a big crew: two-line rows always, the list scrolls; a chip for the ask out of view; ↑ / ↓ move the pick", async ({ cue, page }) => {
    const list = page.locator(".crew-page .cp-scroll");
    await bigCrew(cue, 5);
    await expect(page.locator(".cp-jump")).toBeHidden();
    await bigCrew(cue, 20);
    // Twenty keep their two lines (its word, time and action stay); the list scrolls instead.
    await expect(list.locator(".cp-ml").first()).toBeVisible();
    await expect(list.locator(".cp-act").first()).toBeVisible();
    expect(await list.evaluate((el) => el.scrollHeight > el.clientHeight), "the list scrolls").toBe(true);
    const chip = page.locator(".cp-jump");
    await expect(chip).toHaveText("↓ m19 needs you");
    // The chip scrolls to it; nothing is picked.
    await chip.click();
    await expect(page.locator('.cp-card[data-key="cpc:lead|id-m19"]')).toBeInViewport();
    await expect(chip).toBeHidden();
    await expect(page.locator(".cp-lead")).toHaveClass(/\bon\b/);
    // ↑ / ↓ from The lead's row: the pick walks the list's order, the cursor staying in the list.
    await page.locator(".cp-lead").focus();
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(".cpm .cpv-name")).toHaveText("m0");
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(".cpm .cpv-name")).toHaveText("m1");
    await expect(page.locator('.cp-row[data-cp-mate="lead|id-m1"]')).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(page.locator(".cpm .cpv-name")).toHaveText("m0");
    await page.keyboard.press("Home");
    await expect(page.locator(".cpm")).toHaveCount(0);
    await expect(page.locator(".cp-lead")).toBeFocused();
  });

  // Narrow (SPEC §13): at the smallest window a teammate keeps its chips and three tabs; the bar keeps the command.
  test("at the smallest window: a teammate's files stay chips over its three tabs, its steps under its card; the bar drops its keys and reason, not the command", async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 600 });
    await expect(page.locator(".cp-bar .cpb-keys")).toBeHidden();
    await expect(page.locator(".cp-bar .cpb-cmd")).toBeVisible();
    await page.locator('.cp-row[data-cp-mate="lead|id-handoff"]').click();
    const v = page.locator(".cpm");
    await expect(v.locator(".cpm-tabs button:visible")).toHaveText(["Diff so far", "Full review", "Full task"]);
    await expect(page.locator(".cp-exp .cpm-st").filter({ hasText: "Started" })).toBeVisible();
    await v.locator('[data-cp-tab="task"]').click();
    await v.locator('.cpm-chip[data-cp-file="ui/core.js"]').click();
    // Its whole diff over the page, wider than the left.
    const lines = page.locator(".dov-lines");
    await expect(lines.locator(".a").first()).toHaveText("+# core.js");
    const wide = await lines.evaluate((el) => el.clientWidth);
    await page.keyboard.press("Escape");
    await expect(v.locator('.cpm-tabs [data-cp-tab="diff"]'), "a file picked: its diff").toHaveClass(/\bon\b/);
    await expect(v.locator(".cpm-diff .add").first()).toHaveText("+# core.js");
    // The diff has the left's width less its margins, not a side column's share.
    const [left, diff] = await Promise.all([page.locator(".cp-left").evaluate((el) => el.clientWidth), v.locator(".cpm-diff").evaluate((el) => el.clientWidth)]);
    expect(left - diff).toBeLessThan(40);
    expect(wide).toBeGreaterThan(left);
  });

  // Keys (SPEC §4): a bare letter is always text here; ⌘⇧A / ⌘⇧D answer the ask the bar shows, never a one-way one.
  test("letters answer nothing on the page; ⌘⇧A allows the shown ask, ⌘⇧↓ steps, a one-way ask takes no key", async ({ cue, page }) => {
    await page.locator(".cp-left .ap-chat").first().click({ position: { x: 20, y: 20 } });
    for (const k of ["a", "d", "1"]) await page.keyboard.press(k);
    expect(await cue.calls("respond"), "A, D and 1 are only text here").toHaveLength(0);
    await page.keyboard.press("r");
    await expect(page.locator('.cp-left textarea[data-text="s:lead"]'), "R: the cursor to the box").toBeFocused();
    const hub0 = await page.evaluate(() => state.items.find((i) => i.id === "n1793984390004"));
    await page.keyboard.press("Meta+Shift+A");
    expect((await cue.called("respond", { id: "n1793984390004" })).args.decision.behavior).toBe("allow");
    // A one-way ask: no key, said in words.
    await cue.change((st, hub) => {
      st.items = st.items.filter((i) => i.id !== hub.id);
      st.items.push({ ...hub, id: "own1", agent_id: "", agent_name: "", agent_pid: 0, tool_input: { command: "git push origin main" }, one_way: true });
      st.items.push({ ...hub, id: "rows1", agent_id: "id-rows", agent_name: "rows", tool_input: { command: "npm run migrate" }, created_ms: hub.created_ms + 5 });
    }, hub0);
    await expect(page.locator(".cp-bar .cpb-who")).toHaveText("The lead");
    await page.waitForTimeout(650);
    await page.keyboard.press("Meta+Shift+A");
    await page.keyboard.press("Meta+Shift+D");
    expect(await cue.calls("respond"), "nothing answered the one-way ask").toHaveLength(1);
    await page.keyboard.press("Meta+Shift+ArrowDown");
    await expect(page.locator(".cp-bar .cpb-who")).toHaveText("rows");
    await page.keyboard.press("Meta+Shift+D");
    expect(await cue.calls("respond"), "inside the moment after the change: nothing").toHaveLength(1);
    await page.waitForTimeout(650);
    await page.keyboard.press("Meta+Shift+D");
    expect((await cue.called("respond", { id: "rows1" })).args.decision.behavior).toBe("deny");
  });

  test("Esc, nearest first: the box's text keeps it, the chip comes off, the left goes back to the chat, then the page closes; it never stops the lead", async ({ cue, page }) => {
    await cue.change((st) => { st.sessions.find((s) => s.session_id === "lead").state = "working"; });
    const box = page.locator('.cp-left textarea[data-text="s:lead"]');
    await page.locator(".cp-bar [data-cp-chip]").click();
    await box.fill("just the team tests");
    await page.keyboard.press("Escape");
    await expect(box, "the cursor leaves the box").not.toBeFocused();
    await expect(box, "its draft stays").toHaveValue("just the team tests");
    await expect(page.locator(".cp-chip")).toHaveCount(1);
    await page.keyboard.press("Escape");
    await expect(page.locator(".cp-chip"), "then the chip").toHaveCount(0);
    await page.locator(".cp-bar [data-cp-all]").first().click();
    await page.keyboard.press("Escape");
    await expect(page.locator(".cp-alt"), "then the left back to the chat").toHaveCount(0);
    await expect(page.locator(".crew-page")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(box, "back on the chat, the cursor in its box: Esc leaves the box first").not.toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.locator(".crew-page"), "then the page").toHaveCount(0);
    expect(await cue.calls("interrupt_session"), "Esc never stops the lead here").toHaveLength(0);
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
    // The answer's state has come back (its draw lands now, not partway through a test's measuring).
    await page.waitForFunction((id) => !state.items.some((i) => i.id === id), TEAM_ASK);
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

  // Handle it (the default): Cue sends fixes and commits by itself, so no Send fixes or Commit; what it did and
  // decided is listed, what can be undone with Undo this, which asks the lead and leaves the line saying "undoing…".
  test("Handle it: the goal on top, what it decided on its own, and Undo this", async ({ cue, page }) => {
    const tray = page.locator(".crew-tray");
    await expect(tray.locator(".crew-body")).toBeVisible();
    await expect(tray.locator(".cg-box .cg-t")).toHaveText(["Build the team view: hand out hub, rows, skill and handoff", "Each part reviewed, with nothing blocking", "Review, send fixes, commit to crew/ branches", "Pushing to main, real choices"]);
    await expect(tray.locator(".mt-grp")).toHaveText(["Needs you · nothing now", "Decisions and steps · 6", "Working"]);
    await expect(tray.locator('[data-mt-act="fixes"], [data-mt-act="commit"]'), "Cue does those itself").toHaveCount(0);
    await expect(tray.locator(".mt-one", { hasText: "handoff" }).locator(".mt-line")).toContainText("reviewed · Cue sends its fixes back");
    await expect(tray.locator(".cd-row .cd-who")).toHaveText(["docs", "handoff", "rows", "spike", "lead", "lead"]);
    // Undo this on the commit, the set-aside and the lead's judgment call; the fixes sent, the hold and the teammate
    // started are steps, with none.
    const undo = await tray.locator(".cd-row").evaluateAll((rows) => rows.map((r) => !!r.querySelector("[data-act=crew-back]")));
    expect(undo).toEqual([true, false, false, true, true, false]);
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
    await expect(docs.locator(".cd-going")).toHaveText("undoing…");
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
    await expect(tray.locator(".mt-grp")).toHaveText(["Needs you · nothing now", "Ready", "Decisions and steps · 6", "Working"]);
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
    await expect(asked.locator(".mg-h")).toHaveText(/^Cue.*Crew$/);
    await expect(asked.locator(".mg-st")).toHaveText(/^✓ Delivered to /);
    await expect(asked.locator(".mg-full")).toHaveCount(0);
    await asked.locator("[data-ask-open]").first().click();
    await expect(asked.locator("[data-ask-open]").first()).toHaveText("− What Cue sent");
    await expect(asked.locator(".mg-full", { hasText: "Review your teammate handoff's work" })).toHaveCount(1);
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

test("a restart opens where you were: Desk and Read come back, not the Board", async ({ cue, page }) => {
  await cue.open("world-team");
  for (const v of ["desk", "read"]) {
    await page.locator(`#app .switch-view [data-view="${v}"]`).click();
    await expect(page.locator(`#app .switch-view [data-view="${v}"]`)).toHaveClass(/\bon\b/);
    await page.reload();
    await expect(page.locator(`#app .switch-view [data-view="${v}"]`), `${v} after a restart`).toHaveClass(/\bon\b/);
  }
});

// #259: a card's leading marks each in a color of their own, in both themes: the project's dot, then the Desk's
// session or crew mark (accent on a card, on-bg on a Working card on the ground), never the name's color.
for (const scheme of ["light", "dark"]) {
  test(`a card's dot and its Desk mark have colors of their own (${scheme})`, async ({ cue, page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    const marks = await page.evaluate(() => [...document.querySelectorAll(".working .card-head")].map((h) => {
      const c = (e) => getComputedStyle(e)[e.matches(".pdot") ? "backgroundColor" : "color"];
      return { dot: c(h.querySelector(".pdot")), mark: c(h.querySelector(".dk-name-ic")), name: c(h.querySelector(".proj")) };
    }));
    expect(marks.length).toBeGreaterThan(0);
    const onBg = await page.evaluate(() => { const e = document.createElement("i"); e.style.color = "var(--on-bg)"; document.body.append(e); const c = getComputedStyle(e).color; e.remove(); return c; });
    for (const m of marks) {
      expect(m.mark).toBe(onBg);
      expect(m.mark).not.toBe(m.dot);
    }
  });
}

test("a session's card under Sessions ends with its size · cache, no bar (#423, #592)", async ({ cue, page }) => {
  await cue.open("board-waiting");
  const card = page.locator(".working[data-session]").first();
  const sid = await card.getAttribute("data-session");
  await cue.change((st, id) => { st.insight = { ...(st.insight || {}), [id]: { tokens: 126000, until_ms: Date.now() + 52 * 60000 - 2000 } }; }, sid);
  const line = page.locator(`.working[data-session="${sid}"] .dk-size`);
  await expect(line).toHaveText("126k · cache 52m");
  await expect(line.locator(".dk-meter")).toHaveCount(0);
});

test.describe("Settings → Privacy", () => {
  test("your names and emails save as a list, one per line or comma-separated, and load back (#595)", async ({ cue, page }) => {
    await cue.open("board-waiting");
    await page.keyboard.press(",");
    const box = page.locator(".sheet [data-owner-names]");
    await expect(box).toHaveValue("");
    await box.fill("Pat Example\npat@example.com, Pat E.\n\n");
    await box.blur();
    await cue.called("set_setting", { key: "owner.names", value: ["Pat Example", "pat@example.com", "Pat E."] });
    await page.keyboard.press("Escape");
    await page.keyboard.press(",");
    await expect(page.locator(".sheet [data-owner-names]")).toHaveValue("Pat Example\npat@example.com\nPat E.");
    await expect(page.locator(".sheet [title]")).toHaveCount(0);
  });
});
