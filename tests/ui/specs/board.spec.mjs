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
  await expect(tray.locator(".crew-head")).toContainText("Cue Crew · 5 · hub needs you · 2 ready · 1 working");
  await expect(tray.locator(".mt-grp").first()).toHaveText("Needs you");
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

// How work goes out to a hosted lead's team (ui/team_send.js; world-team: lead "lead", its teammate hub asking).
test.describe("a hosted lead's crew: the Crew switch, /crew, a task dropped on the lead", () => {
  const TEAM_ASK = "n1793984390004";
  test.beforeEach(async ({ cue, page }) => {
    await cue.open("world-team");
    await openCard(page, TEAM_ASK);
    await page.locator('.mt-row.need [data-act="allow"]').click();
  });
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
    await expect(page.locator(".team-sw").nth(1).locator("button.on")).toHaveText("Handle it");
    await page.locator('[data-crew-commit="lead"][data-crew-mode="wait"]').click();
    expect((await cue.called("crew_commit", { sessionId: "lead" })).args.mode).toBe("wait");
    await expect(page.locator(".crew-foot")).toContainText("waits for your Commit or Send fixes");
    await expect(tray.locator('[data-crew-menu="lead"]')).toHaveText("Ask me ▾");
    const menu = await page.locator(".crew-menu").boundingBox(), line = await tray.locator(".crew-head").boundingBox();
    expect(menu.y + menu.height, "it opens upward, over the chat").toBeLessThanOrEqual(line.y);
    await page.locator(".ap-chat").first().click({ position: { x: 20, y: 20 } });
    await expect(page.locator(".crew-menu")).toHaveCount(0);
  });

  test("the crew is a tray on the box: one edge with it; its line, its caret and Esc toggle it", async ({ page }) => {
    // Opened by itself: handoff and docs are ready (they wait on you).
    const tray = page.locator(".crew-tray");
    await expect(tray.locator(".crew-body")).toBeVisible();
    await expect(tray.locator(".crew-head")).toHaveText(/^Cue Crew · 5 · 2 ready · 2 working\s*Handle it ▾\s*▾$/);
    await expect(tray.locator(".crew-head .dot-live"), "a live dot while any member works").toHaveCount(1);
    await expect(tray.locator(".mt-grp")).toHaveText(["Ready", "Working"]);
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
    await expect(tray.locator(".crew-head")).toHaveText(/^Cue Crew · 5 · 2 ready · 2 working\s*Handle it ▾\s*▴$/);
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
    const body = page.locator(".crew-tray .crew-body");
    await page.locator(".crew-tray .crew-car").click();
    await cue.change((st) => { const lead = st.sessions.find((s) => s.session_id === "lead"); lead.team.find((t) => t.name === "rows").step = "Editing ui/a.js"; });
    await expect(page.locator(".crew-tray")).toBeVisible();
    await expect(body, "nothing new waits: stays closed").toHaveCount(0);
    await cue.change((st) => { const lead = st.sessions.find((s) => s.session_id === "lead"); lead.team.find((t) => t.name === "skill").state = "reviewed"; });
    await expect(body, "skill is ready: open again").toBeVisible();
  });

  test("rows agree with their buttons; done members fold into N done; a name opens its menu; no Set aside", async ({ cue, page }) => {
    const tray = page.locator(".crew-tray");
    const handoff = tray.locator(".mt-row", { hasText: "handoff" });
    await expect(handoff.locator(".mt-n")).toHaveText("handoff2 findings · fixes go back, round 1 of 2");
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
    await expect(tray.locator(".mt-grp").first()).toHaveText("Needs you");
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
