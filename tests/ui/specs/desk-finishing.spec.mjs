// The Desk: an item finishing (Landing, install, a failed build), waiting rows, the panel, and the list's look.
import { test, expect, pick, openDesk } from "./cue.mjs";
import { NOW } from "./clock.mjs";
import { listOrder, boxOf } from "./desk.mjs";

// Done means it's in your app (#181): a DONE is committed, built and installed before it's yours to check.
/** Item `id` of the cue Desk changed by `patch` (merged in). */
const patchItem = (cue, id, patch) => cue.change((st, [k, p]) => { Object.assign(st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === k), p); }, [id, patch]);

test("a finishing item: amber and steady, out of In progress into Landing, then Waiting to install with the install's time and Install now in its head; not waiting on you (#181, #190, #346)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const began = (await page.evaluate(() => now())) - 3 * 60_000;
  const since = await page.evaluate((t) => clockAt(t), began);
  await patchItem(cue, "d5", { state: "finishing", finish: { step: "line", wait: "The main checkout is in the middle of a merge", at: began } });
  const row = page.locator('.dk-items [data-desk-item="d5"]');
  await expect(row).toHaveClass(/\bdk-fin\b/);
  // Its own lane, the landing line, numbered; its row says what it waits on, short, after its holder (#346).
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "# Ready to check", "d2", "# Landing", "d5", "# In progress", "d3", "# Queue", "d4", "d6", "# Done", "d7", "d8", "d9"]);
  await expect(page.locator('.dk-items .dk-gh[data-lane="landing"] .dk-split')).toHaveText("one at a time");
  await expect(row.locator(".dk-rn")).toHaveText("1");
  await expect(row.locator(".dk-who")).toHaveText("host-lead · Waits: The main checkout is in the middle of a merge · 3m");
  // Steady: no blinking dot; amber, not the red of an ask.
  await expect(row.locator(".dk-si .dk-blink")).toHaveCount(0);
  const amber = await page.evaluate(() => { const e = document.createElement("i"); e.style.color = "var(--amber)"; document.body.append(e); const c = getComputedStyle(e).color; e.remove(); return c; });
  await expect(row.locator(".dk-fstep")).toHaveCSS("color", amber);
  // Only its step: its title keeps the ink of any row.
  await expect(row.locator(".dk-t")).not.toHaveCSS("color", amber);
  await expect(page.locator(".dk-items .dk-instnow")).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-next")).toHaveCount(0);
  // Not counted as waiting on you.
  await expect(page.locator(".dk-items .dk-gh.you .n")).toHaveText("1");

  // On main, in the next install (#190, #339): Waiting to install, its head says when, counting down, with one Install
  // now for all of it; the Items head has none, nor the row.
  const due = (await page.evaluate(() => now())) + 30 * 60_000;
  await cue.change((st, t) => { st.desk.install_at = t; }, due);
  await patchItem(cue, "d5", { finish: { step: "queued", commit: "abc9999", at: Date.now() } });
  const clock = await page.evaluate((t) => clockAt(t), due);
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "# Ready to check", "d2", "# Waiting to install", "d5", "# In progress", "d3", "# Queue", "d4", "d6", "# Done", "d7", "d8", "d9"]);
  await expect(row.locator(".dk-who")).toHaveText("host-lead · On main · abc9999");
  await expect(row.locator(".dk-rn")).toHaveCount(0);
  await expect(row.locator('[data-desk-act="install_now"]')).toHaveCount(0);
  // Done, skip install on its row (#490), no hover text.
  await expect(row.locator('[data-desk-act="skip_install"]')).toHaveText("Done, skip install");
  await expect(row.locator('[data-desk-act="skip_install"]')).not.toHaveAttribute("title", /.*/);
  await expect(page.locator(".dk-items .dk-ch .dk-instnow, .dk-items .dk-ch .dk-next")).toHaveCount(0);
  const head = page.locator('.dk-items .dk-gh[data-lane="install"]');
  await expect(head.locator(".dk-next")).toHaveText(`${clock} · in 30m`);
  // Counting down: due in under a minute, it says the seconds left, then what it waits on once it's due.
  await cue.change((st, t) => { st.desk.install_at = t; }, due - 30 * 60_000 + 2_000);
  await expect(head.locator(".dk-next")).toHaveText(/ · in 2s$/);
  await page.clock.runFor(1000);
  await expect(head.locator(".dk-next")).toHaveText(/ · in 1s$/);
  await page.clock.runFor(1500);
  await expect(head.locator(".dk-next")).toHaveText("once no session is mid-turn");
  await cue.change((st, t) => { st.desk.install_at = t; }, due);
  await expect(head.locator(".dk-instnow")).toHaveText("Install now");
  await expect(head.locator(".dk-instnow")).not.toHaveAttribute("title", /.*/);
  // Its page: the Finishing pill, the step in full, Install now among its buttons.
  await row.locator(".dk-t").click();
  const pane = page.locator(".dk-right .dk-pg");   // the item's page, its panel beside its chat (#418)
  await expect(pane.locator(".dk-pnow .dk-pword")).toHaveText("Finishing");
  await expect(pane.locator(".dk-pnow")).toContainText(`On main · installs at ${clock} · in 30m`);
  await expect(pane.locator('.dk-acts [data-desk-act="install_now"]')).toHaveText("Install now");
  await expect(pane.locator('[data-desk-act="looks_right"]')).toHaveCount(0);
  await cue.override("desk_item_act", () => null);
  await pane.locator('.dk-acts [data-desk-act="install_now"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "install_now" });
  await pane.locator('.dk-acts [data-desk-act="skip_install"]').click();
  await cue.called("desk_item_act", { id: "d5", act: "skip_install" });
  // Clicking it doesn't close the page it's on.
  await expect(page.locator(".dk-right .dk-pg h1")).toContainText("#d5");

  await patchItem(cue, "d5", { finish: { step: "building", commit: "abc9999", at: began } });
  await expect(pane.locator(".dk-pnow")).toContainText(`On main · building Cue · since ${since}`);
  await expect(page.locator('[data-desk-act="install_now"], [data-desk-act="skip_install"]')).toHaveCount(0);
  await page.locator(".dk-back").click();
  await expect(row.locator(".dk-who")).toHaveText("host-lead · Building Cue");
  await patchItem(cue, "d5", { finish: { step: "installing", commit: "abc9999", at: began } });
  await expect(row.locator(".dk-who")).toHaveText("host-lead · Installing Cue");
  await patchItem(cue, "d5", { finish: { step: "starting", commit: "abc9999", at: began } });
  await expect(row.locator(".dk-who")).toHaveText("host-lead · Cue is restarting");
  expect(await listOrder(page)).toContain("# Waiting to install");
});

test("the Landing lane is the landing line: the one being checked first, then the one moved up, then the oldest DONE; numbered, each waiting on the one ahead; a failed check goes back to In progress (#346)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const t = (await page.evaluate(() => now())) - 10 * 60_000;
  await patchItem(cue, "d4", { state: "finishing", finish: { step: "line", at: t } });
  await patchItem(cue, "d6", { state: "finishing", finish: { step: "line", at: t + 60_000, first: t + 5 * 60_000 } });
  await patchItem(cue, "d5", { state: "finishing", finish: { step: "checking", at: t + 120_000 } });
  await patchItem(cue, "d3", { state: "finishing", finish: { step: "queued", commit: "c71e0b4feed", at: t } });
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "# Ready to check", "d2", "# Landing", "d5", "d6", "d4", "# Waiting to install", "d3", "# Done", "d7", "d8", "d9"]);
  const n = (id) => page.locator(`.dk-items [data-desk-item="${id}"]`);
  await expect(page.locator('.dk-items .dk-gh[data-lane="landing"] .n')).toHaveText("3");
  expect(await Promise.all(["d5", "d6", "d4"].map((id) => n(id).locator(".dk-rn").textContent()))).toEqual(["1", "2", "3"]);
  await expect(n("d5").locator(".dk-fstep")).toHaveText("Checking · 8m");
  await expect(n("d6").locator(".dk-fstep")).toHaveText("Waits on #d5 · 9m");
  await expect(n("d4").locator(".dk-fstep")).toHaveText("Waits on #d6 · 10m");
  await expect(n("d3").locator(".dk-fstep")).toHaveText("On main · c71e0b4");
  // Its check failed: back with the work, In progress, out of the line; the next one up is first.
  await patchItem(cue, "d5", { finish: { step: "check_failed", at: Date.now(), question: "Its check failed" } });
  expect(await listOrder(page)).toEqual(["# Waiting on you", "d1", "# Ready to check", "d2", "# Landing", "d6", "d4", "# Waiting to install", "d3", "# In progress", "d5", "# Done", "d7", "d8", "d9"]);
  await expect(n("d6").locator(".dk-rn")).toHaveText("1");
  await expect(n("d6").locator(".dk-fstep")).toHaveText("Next to land · 9m");
  await expect(n("d5").locator(".dk-who")).toHaveText(/^Check failed · since /);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((th) => document.documentElement.setAttribute("data-theme", th), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-items").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-346-${theme}.png` });
  }
});

test("a failed build waits on you with its question; its page has the log's last lines in mono, Show all opens the rest (#181)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const log = Array.from({ length: 20 }, (_, n) => `line ${n + 1}`).join("\n");
  await patchItem(cue, "d5", { state: "needs_you", latest: "The build failed", finish: { step: "failed", question: "The build failed", log, at: Date.now() } });
  const row = page.locator('.dk-items [data-desk-item="d5"]');
  await expect(row.locator(".dk-h")).toHaveText("The build failed");
  await expect(page.locator(".dk-items .dk-gh.you .n")).toHaveText("2");
  await row.locator(".dk-t").click();
  const pane = page.locator(".dk-right .dk-pg");   // the item's page, its panel beside its chat (#418)
  await expect(pane.locator(".dk-chain")).toContainText("The build failed");
  const pre = pane.locator(".dk-flog");
  await expect(pre).toHaveText(Array.from({ length: 6 }, (_, n) => `line ${n + 15}`).join("\n"));
  await expect(pre).toHaveCSS("font-family", /mono|Menlo|Courier/i);
  await pane.locator(".dk-flog-more").click();
  await expect(pre).toHaveText(log);
  await expect(pre).toHaveCSS("overflow-y", "auto");
  await expect(pane.locator(".dk-flog-more")).toHaveText("Show less");
  // No "Try again": the waits box says what stopped it and offers Send back to fix, prefilled (#250).
  await expect(pane.locator('[data-desk-act="retry_finish"]')).toHaveCount(0);
  await expect(pane.locator(".dk-wo")).toContainText("The build failed");
  await pane.locator('.dk-wo [data-desk-act="fix_back"]').click();
  await expect(pane.locator('textarea[data-text="back:d5"]')).toHaveValue("The landing stopped: The build failed. Fix it, then say DONE again.");
});

test("Looks right on an item committed as it finished only closes it: its commit in its state line, never \"waiting for install\" (#181)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.installed_ms = 1793984280000 - 60000; });
  await openDesk(page);
  await patchItem(cue, "d2", { files: ["ui/a.js"], commit: "abc9999", finish: { step: "installed", commit: "abc9999", at: Date.now() } });
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-who")).toHaveText("web-lead");
  await expect(row.locator(".dk-ok")).toHaveCount(1);
  await row.locator(".dk-t").click();
  const pane = page.locator(".dk-right .dk-pg");   // the item's page, its panel beside its chat (#418)
  await expect(pane.locator(".dk-pnow .dk-pword")).toHaveText("Ready to check");
  await expect(pane.locator(".dk-panel[data-side-for] .dk-pmeta")).toContainText("with web-lead · committed abc9999");
  // Ready to check is a calm sage dot, not the maroon of an ask (#326).
  const dot = pane.locator(".dk-pnow .dk-pdot");
  await expect(dot).toHaveClass(/\bcheck\b/);
  const [bg, deny] = await dot.evaluate((e) => { const p = document.createElement("i"); p.style.background = "var(--deny)"; e.after(p); const r = [getComputedStyle(e).backgroundColor, getComputedStyle(p).backgroundColor]; p.remove(); return r; });
  expect(bg).not.toBe(deny);
  await expect(pane.locator(".dk-chk-in")).toContainText("In the installed Cue");
  await expect(pane.locator('[data-desk-act="looks_right"]')).toHaveCount(1);
  // The hub only closes it (nothing more committed).
  await cue.override("desk_looks_right", ({ id }) => {
    const it = CueBridge.world.state.desk.projects[0].items.find((i) => i.id === id);
    Object.assign(it, { state: "done", done_at: Math.floor(Date.now() / 1000) });
    CueBridge.emit("state", CueBridge.world.snapshot());
    return { commit: it.commit, files: 0, waiting: [] };
  });
  await pane.locator('[data-desk-act="looks_right"]').click();
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator(".toast").last()).toHaveText("Committed abc9999");
  await page.locator(".dk-back").click();   // the Items list, behind ‹ Items while the item is open (#418)
  await expect(row).toHaveClass(/\bdk-done\b/);
});

test("an item to check from before (files, no commit) keeps today's: waiting for install till Cue has it, then Commit to main commits its files (#181, #247)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").files = ["ui/send.js", "ui/styles/send.css"]; s.desk.installed_ms = 1793984280000 - 60000; });
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(row.locator(".dk-who")).toContainText("waiting for install");
  await expect(row.locator(".dk-ok")).toHaveCount(0);
  await cue.change((s) => { s.desk.installed_ms = 1793984280000 + 60000; });
  await expect(row.locator(".dk-who")).not.toContainText("waiting for install");
  // Not on main yet: no ✓; its page's Commit to main commits its files (#247).
  await expect(row.locator(".dk-ok")).toHaveCount(0);
  await expect(row.locator(".dk-cm")).toHaveCount(0);
  await row.click();
  await page.locator('.dk-panel[data-side-for] .dk-pnow [data-desk-act="looks_right"]:has-text("Commit to main")').click();
  await cue.called("desk_looks_right", { id: "d2" });
  await expect(page.locator(".toast").last()).toContainText("Committed abc1234 · 2 files");
});

test("each waiting row says what to do in one line, and what it unblocks; the most-unblocking first; a stopped item waits on the lead, not on you (#182)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const items = st.desk.projects.find((x) => x.project === "cue").items;
    const it = (id) => items.find((i) => i.id === id);
    // d2 waits on a pick, and four items wait on it (one through another).
    Object.assign(it("d2"), { state: "needs_you", ask: { item_id: "a2", sid: "web-lead", text: "Phrases left or right of Send?", choices: ["Left", "Right"], kind: "question" } });
    st.items.push({ ...st.items.find((i) => i.id === "n-focus-ask"), id: "a2", tool_input: { questions: [{ question: "Phrases left or right of Send?", options: [{ label: "Left" }, { label: "Right" }] }] } });
    Object.assign(it("d4"), { after: "d2" });
    Object.assign(it("d6"), { after: "d4" });
    items.push({ ...it("d6"), id: "d10", title: "Ten", after: "d2" }, { ...it("d6"), id: "d11", title: "Eleven", after: "d10" });
    // d1's ask is a tool to allow.
    Object.assign(it("d1").ask, { kind: "permission", text: "Run npm test", choices: [] });
    // d3's holder stopped on it with no ask: the lead settles it.
    Object.assign(it("d3"), { stopped: true });
  });
  const you = page.locator(".dk-items .dk-gh.you");
  await expect(you).toContainText("2");
  const order = await listOrder(page);
  expect(order.slice(order.indexOf("# Waiting on you"), order.indexOf("# Waiting on you") + 3)).toEqual(["# Waiting on you", "d2", "d1"]);
  await expect(page.locator('[data-desk-item="d2"] .dk-h')).toHaveText("Phrases left or right of Send?");
  await expect(page.locator('[data-desk-item="d2"] .dk-ub')).toHaveText("unblocks #d4, #d10, #d6 +1 more");
  await expect(page.locator('[data-desk-item="d1"] .dk-h')).toHaveText("Allow: Run npm test");
  await expect(page.locator('[data-desk-item="d1"] .dk-ub')).toHaveCount(0);
  const d3 = page.locator('[data-desk-item="d3"]');
  await expect(d3).toContainText("stopped · the Desk lead is on it");
  await expect(d3.locator(".dk-h")).toHaveCount(0);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    if (process.env.SHOT_DIR) await page.locator(".dk-items").screenshot({ path: `${process.env.SHOT_DIR}/cue-desk-182-${theme}.png` });
  }
  // Its page: the same line, and what it unblocks.
  await page.locator('[data-desk-item="d2"]').click();
  const head = page.locator(".dk-panel[data-side-for] .dk-pnow");
  await expect(head.locator(".dk-pickl")).toHaveText("Pick: Phrases left or right of Send?");
  await expect(head.locator(".dk-iub")).toHaveText("unblocks #d4, #d10, #d6 +1 more");
  await page.locator(".dk-back").click();
  await page.locator('[data-desk-item="d3"]').click();
  await expect(head.locator(".dk-pword")).toHaveText("Stopped");
});

test("an item whose holder said what it waits on shows waiting on that, not stopped (#249)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const it = st.desk.projects.find((x) => x.project === "cue").items.find((i) => i.id === "d3");
    Object.assign(it, { state: "working", stopped: false, waiting: "I'm waiting on the test run.", latest: "I'm waiting on the test run." });
  });
  const d3 = page.locator('[data-desk-item="d3"]');
  await expect(d3).toContainText("waiting · I'm waiting on the test run.");
  await expect(d3).not.toContainText("stopped");
  await d3.click();
  const head = page.locator(".dk-panel[data-side-for] .dk-pnow");
  await expect(head.locator(".dk-pword")).toHaveText("Waiting");
  await expect(head).toContainText("waits: I'm waiting on the test run.");
});

test("Hide panel folds the panel for that kind of page and is remembered; a narrow card folds it on its own (#200)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1512, height: 945 });
  await cue.open("desk");
  await openDesk(page);
  const pg = page.locator(".dk-right .dk-pg");
  await expect(pg).toHaveAttribute("data-desk-page", "dispatcher");
  await expect(pg).not.toHaveClass(/\bhid\b/);
  await expect(pg.locator(".dk-panel").first()).toBeVisible();
  await pg.locator(".dk-ptog").click();
  await expect(pg).toHaveClass(/\bhid\b/);
  await expect(pg.locator(".dk-ptog")).toHaveText("Show panel");
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("cue.deskPanelHidden")))).toContain("dispatcher");
  // A session's chat is another kind: its panel still shows.
  await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
  await expect(pg).not.toHaveAttribute("data-desk-page", "dispatcher");
  await expect(pg).not.toHaveClass(/\bhid\b/);
  await expect(pg.locator(".dk-ptog")).toHaveText("Hide panel");
  // An item's page has its own panel beside its chat, and its own Hide panel (#418).
  await page.locator(".dk-back").click();
  await page.locator('[data-desk-item="d4"]').click();
  await expect(page.locator(".dk-panel[data-side-for]")).toBeVisible();
  await expect(pg.locator(".dk-ptog")).toHaveText("Hide panel");
  // The Dispatcher again: still folded; Show panel brings it back.
  await page.locator(".dk-back").click();
  await expect(pg).toHaveAttribute("data-desk-page", "dispatcher");
  await expect(pg).toHaveClass(/\bhid\b/);
  await pg.locator(".dk-ptog").click();
  await expect(pg).not.toHaveClass(/\bhid\b/);
  await expect(pg.locator(".dk-ptog")).toHaveText("Hide panel");
  // A narrow card folds the panel by itself; its toggle goes.
  await page.setViewportSize({ width: 1280, height: 800 });
  await expect(pg.locator(".dk-ptog")).toBeHidden();
});

test("a stopped item's row, open or not, keeps its full-size icon and its kind, number and age under the title (#218)", async ({ cue, page }) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  await expect(page.locator('.dk-items [data-desk-item="d3"]')).toBeVisible();
  await page.evaluate(() => {
    for (const d of state.desk?.projects || []) for (const i of d.items || []) if (i.id === "d3") { i.stopped = true; i.title = 'Desk header: drop "Delete Desk…", show the desk icon instead of the word "Desk", line up the name and sessions line'; }
    renderMain();
  });
  const row = page.locator('.dk-items [data-desk-item="d3"]');
  const shape = () => row.evaluate((el) => {
    const si = el.querySelector(".dk-si").getBoundingClientRect(), i1 = el.querySelector(".dk-i1").getBoundingClientRect(), i2 = el.querySelector(".dk-i2").getBoundingClientRect();
    return { icon: [si.width, si.height], lines: Math.round(i1.height / parseFloat(getComputedStyle(el.querySelector(".dk-i1")).lineHeight)), under: i2.top >= i1.bottom - 1, sameLeft: Math.abs(i2.left - i1.left) < 1 };
  });
  await expect(row).toHaveClass(/dk-stop/);
  expect(await shape()).toEqual({ icon: [14, 14], lines: 3, under: true, sameLeft: true });
  // Opened, the list goes behind ‹ Items (#418); back, the row is as it was.
  await row.click();
  await expect(page.locator('.dk-panel[data-side-for="d3"]')).toBeVisible();
  await page.locator(".dk-back").click();
  await expect(row).toBeVisible();
  expect(await shape()).toEqual({ icon: [14, 14], lines: 3, under: true, sameLeft: true });
});

test("the Desk list's › sits only on the open row or card and moves with it, in both themes (#219)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const left = page.locator(".dk-sessions");
  const arrows = left.locator(".dk-larrow, .dk-go");
  // host-lead not working: on a working session's chat Esc is its Stop first (#494, desk-esc.spec.mjs).
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").state = "waiting"; });
  const owner = () => arrows.evaluateAll((els) => els.map((el) => { const r = el.closest(".dk-srow, .dk-lcard, .dk-holder"); return [r.dataset.deskLeadChat || r.dataset.deskChatOf || r.dataset.deskHolder || r.dataset.deskAddon, r.classList.contains("on")]; }));
  for (const scheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: scheme });
    // The lead's chat shows: the Run by card alone has it.
    await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
    expect(await owner()).toEqual([["lead", true]]);
    // Open another session: the › goes with it, none on the Run by card.
    await left.locator('[data-desk-chat-of="host"]').click();
    await expect(left.locator('[data-desk-chat-of="host"]')).toHaveClass(/\bon\b/);
    expect(await owner()).toEqual([["host", true]]);
    await expect(left.locator('[data-desk-lead-chat="lead"] .dk-larrow')).toHaveCount(0);
    // Back to the lead's: back on its card.
    await page.keyboard.press("Escape");
    await expect(left.locator('[data-desk-lead-chat="lead"]')).toHaveClass(/\bon\b/);
    expect(await owner()).toEqual([["lead", true]]);
  }
});

test("the Desk's session list: one hairline above each session but a group's first; crew and N done stay in its block; none at the end, open or not (#225)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => {
    const base = st.sessions.find((x) => x.session_id === "site");
    const m = (name, state, extra = {}) => ({ also: [], ended_ms: 0, files: [], summary: "", state, agent_id: `id-${name}`, call: `c-${name}`, name, what: "Check the build", asked: "", started_ms: 1793983560000, ...extra });
    const add = (id, team, since) => st.sessions.push({ ...base, session_id: id, name: id, project: "cue", thread: [], state: "idle", doing: "", team, since_ms: since });
    add("s-a", [m("a1", "committed")], 9);
    add("s-b", [], 8);
    add("s-c", [m("c-live", "reviewed", { uncommitted: true })], 7);
    add("s-d", [m("d-live", "reviewed", { uncommitted: true }), m("d1", "committed"), m("d2", "committed")], 6);
    add("s-e", [], 5);
    st.desk.projects.find((d) => d.project === "cue").members.push("s-a", "s-b", "s-c", "s-d", "s-e");
    // Their caches still warm, so they stay in Idle (a cold one folds under Older, #485).
    st.insight = Object.fromEntries(["s-a", "s-b", "s-c", "s-d", "s-e"].map((sid) => [sid, { tokens: 50000, until_ms: Date.now() + 30 * 60000 }]));
  });
  const left = page.locator(".dk-sessions");
  await expect(left.locator('[data-desk-chat-of="s-e"]')).toHaveCount(1);
  // Each child of the list: what it is, and whether a line is drawn on it (above it).
  const lines = () => left.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => {
    const what = el.matches(".dk-gh") ? "#" : el.matches(".dk-sfold") ? "fold" : el.matches(".crew") ? "crew" : el.dataset.deskChatOf;
    const cs = getComputedStyle(el), line = cs.boxShadow !== "none" || parseFloat(cs.borderTopWidth) > 0 || parseFloat(cs.borderBottomWidth) > 0;
    return `${what}${line ? " —" : ""}`;
  }));
  const want = ["#", "host", "#", "s-a", "fold", "s-b —", "s-c —", "crew", "s-d —", "crew", "fold", "s-e —"];
  for (const scheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: scheme });
    expect(await lines()).toEqual(want);
    // Opening a session leaves the lines as they were, above it and under it.
    await left.locator('[data-desk-chat-of="s-c"]').click();
    await expect(left.locator('[data-desk-chat-of="s-c"]')).toHaveClass(/\bon\b/);
    expect(await lines()).toEqual(want);
    await page.keyboard.press("Escape");
  }
  // No line closing the list: the last row draws none under it.
  expect(await left.locator(".dk-scroll > :last-child").evaluate((el) => [getComputedStyle(el).borderBottomWidth, getComputedStyle(el).boxShadow.includes("-1px")])).toEqual(["0px", false]);
});

test("the Desk panel's buttons are sized to their words; Stop is small and quiet, right under the Now line, and still stops it, in both themes (#245)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 1512, height: 900 });
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const dir = process.env.SHOT_DIR || process.env.TMPDIR || "/tmp";
  await page.evaluate(() => { state.sessions.find((s) => s.name === "host-lead").state = "working"; renderMain(); });
  await page.locator('.dk-sessions .dk-srow:not(.crew)', { hasText: "host-lead" }).first().click();
  const sp = page.locator('.dk-right .dk-pg[data-desk-page="session"]');
  const now = sp.locator(".dk-pnow");
  await expect(now.locator(".dk-pword")).toHaveText("Working");
  const stop = now.locator('[data-act="interrupt"]');
  await expect(stop).toHaveText("Stop");
  for (const scheme of ["light", "dark"]) {
    await page.emulateMedia({ colorScheme: scheme });
    await expect(async () => {
      const m = await stop.evaluate((b) => {
        const c = getComputedStyle(b), r = b.getBoundingClientRect(), p = b.closest(".dk-panel").getBoundingClientRect();
        const say = b.closest(".dk-pnow").querySelector(".dk-psay").getBoundingClientRect();
        return { w: r.width, pw: p.width, h: r.height, bg: c.backgroundColor, border: c.borderTopColor, gap: r.top - say.bottom };
      });
      expect(m.w).toBeLessThan(m.pw / 3);                        // sized to its word, no full-width bar
      expect(m.h).toBeLessThanOrEqual(24);
      expect(m.bg).toBe("rgba(0, 0, 0, 0)");                    // no fill, no border: quiet
      expect(m.border).toBe("rgba(0, 0, 0, 0)");
      expect(m.gap).toBeGreaterThanOrEqual(0);                  // right under the Thinking line
      expect(m.gap).toBeLessThan(14);
    }).toPass();
    await page.screenshot({ path: `${dir}/cue-desk-stop-${scheme}.png`, clip: await page.locator(".dk-right").boundingBox() });
  }
  await page.emulateMedia({ colorScheme: "light" });
  // Every panel button is sized to its words: none spans the panel.
  const wide = await page.locator(".dk-right .dk-panel .dk-pacts .btn").evaluateAll((bs) => bs.filter((b) => b.getBoundingClientRect().width > b.closest(".dk-panel").clientWidth * 0.8).length);
  expect(wide).toBe(0);
  // Folded (a narrow card): Stop sits on the Now line itself.
  await page.setViewportSize({ width: 900, height: 800 });
  const [r1, r2] = await Promise.all([now.locator(".dk-pnr").boundingBox(), stop.boundingBox()]);
  expect(Math.abs((r1.y + r1.height / 2) - (r2.y + r2.height / 2))).toBeLessThan(4);
  expect(r2.width).toBeLessThan(80);
  await stop.click();
  await expect.poll(() => cue.calls("interrupt_session")).toHaveLength(1);
});

test("an item shows ready only once its work is on main: until then it says where it is, and uncommitted work says Commit to main, never Looks right (#247)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // Committed, and not on main yet: its row says where, with no ✓.
  const at = Date.now(), since = await page.evaluate((t) => clockAt(t), at);
  await patchItem(cue, "d5", { state: "finishing", finish: { step: "landing", wait: "on side, not main", at } });
  const d5 = page.locator('.dk-items [data-desk-item="d5"]');
  await expect(d5.locator(".dk-who")).toHaveText("host-lead · Not on main yet · on side, not main · 0s");
  await expect(d5.locator(".dk-ok")).toHaveCount(0);
  // To check with its files uncommitted (its session on another machine): no ✓ on its row; its page says it isn't on
  // main and its button commits it there.
  await patchItem(cue, "d2", { files: ["ui/chat/compose.js"] });
  const d2 = page.locator('.dk-items [data-desk-item="d2"]');
  await expect(d2.locator(".dk-ok")).toHaveCount(0);
  await d2.click();
  const now = page.locator(".dk-panel[data-side-for] .dk-pnow");
  await expect(now.locator(".dk-pword")).toHaveText("Not on main yet");
  await expect(now.locator('[data-desk-act="looks_right"]')).toHaveText("Commit to main");
  // Committed at its DONE and on main: Close, beside its check line (#526).
  await patchItem(cue, "d2", { finish: { step: "checked", commit: "abc1234", at: Date.now() }, commit: "abc1234" });
  await expect(now.locator(".dk-pword")).toHaveText("Ready to check");
  await expect(now.locator('[data-desk-act="looks_right"]')).toHaveText("Close");
  await page.locator(".dk-back").click();   // its row in the Items list, behind ‹ Items while it's open (#418)
  await expect(d2.locator(".dk-ok")).toHaveCount(1);
});

test("the lead's MERGE line reads as a short line: the copy folded into the item it repeats (#321)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.evaluate(() => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: "MERGE: #x9 into #d2 | why: the same ask again" }); renderMain(); });
  const ln = page.locator(".dk-chat .cv-agent").last().locator(".dk-ln");
  await expect(ln).toHaveCount(1);
  await expect(ln.locator(".mg-lh")).toHaveText(/^Folded #x9 into \w+ #d2$/);
  await ln.locator(".mg-line").click();
  await expect(ln.locator(".mg-full")).toContainText("the same ask again");
});

test("the open item's long title keeps its full width beside its icon, not one letter per line (#327)", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((s) => { s.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d2").title = "Loops on the Mac look and work like option G, with the open loop and the Loops list matching the design"; });
  await openDesk(page);
  await page.locator('.dk-items [data-desk-item="d2"]').click();
  // Its page's head (#418: the list is behind ‹ Items while it's open): the title beside its kind and number.
  const h1 = page.locator(".dk-right .dk-pg h1"), title = h1.locator(".dk-tname");
  await expect(title).toContainText("Loops on the Mac");
  const t = await boxOf(title), k = await boxOf(h1.locator(".dk-hk")), r = await boxOf(h1);
  expect(t.w).toBeGreaterThan(r.w * 0.6);
  const lines = await title.evaluate((el) => Math.round(el.getBoundingClientRect().height / parseFloat(getComputedStyle(el).lineHeight || 18)));
  expect(lines).toBeLessThanOrEqual(3);
  expect(k.y).toBeLessThanOrEqual(t.y + 4);   // its kind and number on its first line
});

test("a working row says what its holder does and since when; quiet 10 minutes, it reads Stalled (#285)", async ({ page, cue }) => {
  await cue.open("desk", { frozen: true });
  await openDesk(page);
  const order = await listOrder(page);
  const heights = () => page.locator(".dk-items .dk-it").evaluateAll((els) => els.map((el) => el.getBoundingClientRect().height));
  const before = await heights();
  const set = (acts) => cue.change((st, acts) => {
    const items = st.desk.projects.find((x) => x.project === "cue").items;
    for (const [id, act] of Object.entries(acts)) items.find((i) => i.id === id).act = act;
  }, acts);
  const MIN = 60000;
  const act = (ago, now, was, quiet = 10) => ({ at_ms: NOW - ago * MIN, now, was, stalls_at: NOW - ago * MIN + quiet * MIN });
  // d3 one minute in, d5 fourteen quiet minutes; d1 waits on you, however old its last step.
  await set({ d3: act(1, "Editing desk.rs", "last edited desk.rs"), d5: act(14, "Reading model.rs", "last read model.rs"), d1: act(30, "Running cargo test", "last ran cargo test") });
  await expect(page.locator('[data-desk-item="d3"] .dk-doing')).toHaveText("Editing desk.rs · 1m");
  await expect(page.locator('[data-desk-item="d5"] .dk-doing')).toHaveText("Stalled · last read model.rs · 14m ago");
  await expect(page.locator('[data-desk-item="d5"]')).toHaveClass(/stalled/);
  await expect(page.locator('[data-desk-item="d3"]')).not.toHaveClass(/stalled/);
  const color = (sel) => page.locator(sel).evaluate((el) => getComputedStyle(el).color);
  expect(await color('[data-desk-item="d5"] .dk-t')).not.toEqual(await color('[data-desk-item="d3"] .dk-t'));
  expect(await page.locator('[data-desk-item="d5"]').evaluate((el) => getComputedStyle(el).borderLeftWidth)).toBe("0px");
  await expect(page.locator(".dk-items .dk-gh .dk-split")).toHaveText("1 moving · 1 stalled");
  // Waiting on you is never stalled, nor counted.
  await expect(page.locator('[data-desk-item="d1"] .dk-doing')).toHaveCount(0);
  await expect(page.locator(".dk-items .dk-gh .dk-split")).toHaveCount(1);
  // The boundary: a second before its tenth quiet minute it's moving; at it, stalled.
  await set({ d5: { at_ms: NOW - 9 * MIN, now: "Reading model.rs", was: "last read model.rs", stalls_at: NOW + 1000 }, d3: { at_ms: NOW - 10 * MIN, now: "Editing desk.rs", was: "last edited desk.rs", stalls_at: NOW } });
  await expect(page.locator('[data-desk-item="d5"] .dk-doing')).toHaveText("Reading model.rs · 9m");
  await expect(page.locator('[data-desk-item="d3"] .dk-doing')).toHaveText("Stalled · last edited desk.rs · 10m ago");
  await expect(page.locator(".dk-items .dk-gh .dk-split")).toHaveText("1 moving · 1 stalled");
  // Same rows, same heights, same order.
  expect(await heights()).toEqual(before);
  expect(await listOrder(page)).toEqual(order);
  // With nothing new from the hub, the clock alone turns it: d5's quiet time runs out and it reads Stalled.
  await page.clock.setFixedTime(NOW + 2000);
  await page.clock.runFor(1000);
  await expect(page.locator('[data-desk-item="d5"] .dk-doing')).toHaveText(/^Stalled · last read model.rs · /);
  await expect(page.locator(".dk-items .dk-gh .dk-split")).toHaveText("0 moving · 2 stalled");
});

test("each item shows what you'll see in plain words under its title; its Done when stays the session's; the lead's SEE line reads as one (#382)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const row = page.locator('.dk-items [data-desk-item="d4"]');
  await expect(row.locator(".dk-see")).toHaveText("You'll see: The rail's dot blinks only for a session you wrote to, once its reply comes back.");
  await expect(page.locator(".dk-items")).not.toContainText("desk.spec checks it");
  // One without it yet: no empty line.
  await expect(page.locator(".dk-items .dk-it:not(:has(.dk-see))").first()).toBeVisible();
  await page.evaluate(() => {
    sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: "SEE: #d4 | the dot blinks only for you | done when: rail.spec passes" });
    renderMain();
  });
  const ln = page.locator(".dk-chat .cv-agent").last().locator(".own-lines .dk-ln");
  await expect(ln).toHaveCount(1);
  await expect(ln.locator(".mg-lh")).toContainText("Said what you'll see for");
  // (On its words: the item's number in it is its own button, opening the item.)
  await ln.locator(".mg-line").click({ position: { x: 6, y: 6 } });
  await expect(ln.locator(".mg-full")).toHaveText("You'll see the dot blinks only for you");
  await expect(ln).not.toContainText("rail.spec");
});
