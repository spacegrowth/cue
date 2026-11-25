import { test, expect, openDesk } from "./cue.mjs";
import { openSessions, groupOf, listOrder, statusOf } from "./desk.mjs";

async function setup(cue, page) {
  await cue.open("desk");
  await cue.change((st) => {
    const worker = st.sessions.find((s) => s.session_id === "host");
    worker.harness = "codex"; worker.hosted = true; worker.state = "deciding";
    const desk = st.desk.projects.find((d) => d.project === "cue");
    for (const item of desk.items.filter((i) => i.holder?.sid === "host")) {
      item.state = "working"; item.now_on = false; item.ask = null;
    }
    st.items.push({ id: "codex-approval", session_id: "host", harness: "codex", hosted: true, project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command: "printf approval-check" }, suggestions: [], created_ms: Date.now() });
  });
  await openDesk(page);
}

test("a checked Cue fix without a commit hash waits for verification on the Desk", async ({ cue, page }) => {
  await setup(cue, page);
  await cue.change((st) => {
    st.items = [];
    const desk = st.desk.projects.find((d) => d.project === "cue");
    const base = desk.items.find((i) => i.id === "d2");
    const installed = 1795019695327, edited = 1795019736763, at = 1795019768464;
    st.desk.installed_ms = installed;
    desk.items.push(
      { ...base, id: "265", title: "Send Desk item quick phrases immediately", done_when: "Clicking a quick phrase sends it immediately.", state: "to_check", commit: "", finish: { step: "checked", at }, checks: [{ at, edited, cue: true }] },
      { ...base, id: "awaits-install", title: "Queued for install", state: "finishing", finish: { step: "queued", at }, checks: [{ at, edited, cue: true }] }
    );
  });
  // On the Desk it waits on you to check, not as a question (#602): under WAITING ON YOU, its row with the ✓ (Looks
  // right), the one still installing in its own group under it (#824) with none.
  expect(await groupOf(page, "265")).toBe("WAITING ON YOU");
  await expect(page.locator('[data-desk-item="265"]')).not.toHaveClass(/\bask\b/);
  await expect(page.locator('[data-desk-item="265"] .dk-ok')).toHaveAttribute("aria-label", "Looks right");
  expect(await groupOf(page, "awaits-install")).toBe("WAITING TO INSTALL");
  await expect(page.locator('[data-desk-item="awaits-install"] .dk-ok')).toHaveCount(0);
  await page.locator('[data-desk-item="265"]').click();
  // It opens its session's chat with its bar: Looks right closes it.
  const bar = page.locator(".dk-right .dkn-ib.check");
  await bar.locator('[data-desk-act="looks_right"][data-id="265"]').click();
  await cue.called("desk_looks_right", { id: "265" });
  await page.locator(".dk-right .dkn-sback .dk-back").click();
  // Out of Waiting on you: done today.
  await expect.poll(() => groupOf(page, "265")).toBe("DONE TODAY");
});

test("the Desk counts its open decisions, excluding resolved requests and reviews", async ({ cue, page }) => {
  await setup(cue, page);
  await cue.change((st) => {
    st.items = st.items.filter((i) => ["n-focus-ask", "codex-approval"].includes(i.id));
    const task = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5");
    Object.assign(task, { state: "needs_you", ask: { item_id: "codex-approval", sid: "host", kind: "permission", text: "printf approval-check" } });
  });
  // The Desk's open decisions: the asking rows under WAITING ON YOU (what's ready to check is there too, after them).
  const deskRows = async () => { const o = await listOrder(page), at = o.indexOf("# WAITING ON YOU"); const rows = []; for (const x of o.slice(at + 1)) { if (x.startsWith("# ")) break; rows.push(x); } return rows; };
  const deskAsks = async () => { const out = []; for (const id of await deskRows()) if (await page.locator(`.dk-items [data-desk-item="${id}"]`).evaluate((e) => e.classList.contains("ask"))) out.push(id); return out; };
  const deskCount = async () => (await deskAsks()).length;
  await expect.poll(deskCount).toBe(2);
  expect((await deskAsks()).sort()).toEqual(["d1", "d5"]);
  expect((await deskRows()).slice(0, 2).sort()).toEqual(["d1", "d5"]);
  expect(await groupOf(page, "d2")).toBe("WAITING ON YOU");
  await expect(page.locator('.dk-items [data-desk-item="d2"]')).not.toHaveClass(/\bask\b/);
  await expect(page.locator('[data-desk-request="codex-approval"]')).toHaveCount(0);

  // The card can resolve before refreshed item metadata arrives.
  await cue.change((st) => { st.items.find((i) => i.id === "codex-approval").status = "answered"; });
  await expect.poll(deskCount).toBe(1);
  expect(await deskAsks()).toEqual(["d1"]);
  await expect(statusOf(page, "d5")).not.toContainText("Asks");
});

// No permission cards on the Desk's left list (#431): a session waiting on one has only its small "needs you" mark there;
// the request is on its item on the right and in the session's chat.
test("a request with no item tag puts no card on the left: the session's row reads needs you, and its chat answers it (#431)", async ({ cue, page }) => {
  await setup(cue, page);
  // The sessions (All N sessions, on the right in the list's place, #666).
  await openSessions(page);
  const left = page.locator(".dk-sessions");
  const row = left.locator('[data-desk-chat-of="host"]');
  await expect(row.locator(".dk-st")).toHaveText("needs you");
  await expect(row.locator(".dk-st > i.ask")).toHaveCount(1);
  // Fails if a permission card ever renders in the left list.
  await expect(left.locator('[data-desk-request], .dk-request, .dk-approvals, [data-act="allow"], [data-desk-decision]')).toHaveCount(0);
  await expect(left).not.toContainText("Needs your decision");
  await expect(left).not.toContainText("printf approval-check");
  expect(await left.locator("[title]").count()).toBe(0);
  await row.click();
  await expect(page.locator(".dk-right .dk-ask")).toContainText("printf approval-check");
  await page.locator('.dk-right [data-act="allow"][data-id="codex-approval"]').click();
  await cue.called("respond", { id: "codex-approval", decision: { behavior: "allow" } });
  await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
  // ‹ All sessions: the list again, its row no longer needing you.
  await page.locator(".dk-right .dk-back", { hasText: "All sessions" }).click();
  await expect(row.locator(".dk-st")).not.toHaveText("needs you");
});

test("a request on an item shows on the item and in its session's chat, never as a card on the left (#431)", async ({ cue, page }) => {
  await setup(cue, page);
  await cue.change((st) => {
    const task = st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5");
    Object.assign(task, { state: "needs_you", ask: { item_id: "codex-approval", sid: "host", kind: "permission", text: "printf approval-check" } });
  });
  const left = page.locator(".dk-sessions");
  // Its row says it asks you; its chat has the card that answers it.
  await expect(statusOf(page, "d5")).toHaveText("Asks you");
  await page.locator('.dk-items [data-desk-item="d5"]').click();
  await expect(page.locator('.dk-right [data-act="allow"][data-id="codex-approval"]')).toHaveCount(1);
  await page.locator(".dk-right .dkn-sback .dk-back").click();
  await openSessions(page);
  await expect(left.locator('[data-desk-chat-of="host"] .dk-st')).toHaveText("needs you");
  await expect(left.locator('[data-desk-request], .dk-request, .dk-approvals, [data-act="allow"], [data-desk-decision]')).toHaveCount(0);
  await left.locator('[data-desk-chat-of="host"]').click();
  await expect(page.locator(".dk-right .dk-ask")).toContainText("printf approval-check");
});

test("the worker chat on Desk has the live approval footer and Deny clears it", async ({ cue, page }) => {
  await setup(cue, page);
  await openSessions(page);
  await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
  await expect(page.locator(".dk-right .dk-ask")).toContainText("printf approval-check");
  await page.locator('.dk-right [data-act="deny"][data-id="codex-approval"]').click();
  await cue.called("respond", { id: "codex-approval", decision: { behavior: "deny" } });
  await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
});

test("Allow from the session removes the same pending request from Desk", async ({ cue, page }) => {
  await setup(cue, page);
  await openSessions(page);
  // The session on the Board (its ask in its Desk's card, #825), answered there.
  await page.locator('.switch-view [data-view="board"]').click();
  await page.locator('.dkw-ask[data-big="codex-approval"]').click();
  await page.locator('.active-pane [data-act="allow"][data-id="codex-approval"]').click();
  await cue.called("respond", { id: "codex-approval" });
  await page.locator('.switch-view [data-view="desk"]').click();
  await expect(page.locator('.dk-right [data-id="codex-approval"]')).toHaveCount(0);
  await openSessions(page);
  await expect(page.locator('.dk-sessions [data-desk-chat-of="host"] .dk-st')).not.toHaveText("needs you");
});

for (const choice of ["Allow", "No"]) {
  test(`Computer Use confirmation shows right-aligned Allow and No in the session's chat and ${choice} answers it`, async ({ cue, page }) => {
    await setup(cue, page);
    await cue.change((st) => {
      const request = st.items.find((it) => it.id === "codex-approval");
      request.kind = "input";
      request.tool_name = "cua_repl";
      request.tool_input = { server: "cua_repl", message: 'Allow Computer Use to use "Cue"?', form: { type: "object", properties: {} } };
    });
    await openSessions(page);
    await expect(page.locator('.dk-sessions [data-desk-request], .dk-sessions .dk-request')).toHaveCount(0);
    await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
    const footer = page.locator(".dk-right .dk-ask");
    await expect(footer).toContainText('Allow Computer Use to use "Cue"?');
    await expect(footer.locator(".row-btns button")).toHaveText(["No", "Allow"]);
    const bounds = await footer.locator(".act-card").boundingBox(), right = await footer.getByRole("button", { name: "Allow", exact: true }).boundingBox();
    expect(bounds.x + bounds.width - right.x - right.width).toBeLessThan(20);
    await footer.getByRole("button", { name: choice, exact: true }).click();
    await cue.called("respond", { id: "codex-approval", decision: choice === "Allow" ? { behavior: "allow", answers: {} } : { behavior: "deny" } });
    await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
  });
}

const PLAYWRIGHT_227 = "node node_modules/@playwright/test/cli.js test --config tests/ui/playwright.config.mjs --grep 'a Desk can start in a folder|the lead.s card in the strip|three columns'";

for (const [verb, behavior] of [["Allow", "allow"], ["No", "deny"]]) {
  test(`Feature #227 Playwright approval appears under its task and ${verb} answers it`, async ({ cue, page }) => {
    await setup(cue, page);
    await cue.change((st, command) => {
      const d = st.desk.projects.find((desk) => desk.project === "cue");
      const task = d.items.find((item) => item.holder?.sid === "host");
      Object.assign(task, { id: "227", kind: "feature", title: "Give the Desk its own icon" });
      const request = st.items.find((item) => item.id === "codex-approval");
      request.tool_input = { command };
      request.thread = [{ role: "you", text: "Desk item #227 (feature): Give the Desk its own icon" }];
    }, PLAYWRIGHT_227);
    const task = page.locator('.dk-items [data-desk-item="227"]');
    await expect(task).toHaveClass(/\bask\b/);
    expect(await groupOf(page, "227")).toBe("WAITING ON YOU");
    // Its row says it asks you; its chat's card answers it.
    await expect(statusOf(page, "227")).toHaveText("Asks you");
    await expect(page.locator('.dk-sessions [data-desk-request="codex-approval"]')).toHaveCount(0);
    await task.click();
    const approval = page.locator('.dk-right .act-card').filter({ has: page.locator('[data-id="codex-approval"]') });
    await approval.locator(`[data-act="${behavior}"][data-id="codex-approval"]`).click();
    await cue.called("respond", { id: "codex-approval", decision: { behavior } });
    await expect(approval).toHaveCount(0);
  });
}

test("a crew member's approval, whose brief has no item tag, waits under its session's item on the right, not on the left (#299)", async ({ cue, page }) => {
  await setup(cue, page);
  await cue.change((st) => {
    const d = st.desk.projects.find((desk) => desk.project === "cue");
    const task = d.items.find((item) => item.holder?.sid === "host");
    Object.assign(task, { id: "295", kind: "task", title: "New designs for an open loop", state: "working" });
    for (const other of d.items) if (other !== task && (other.holder?.sid === "host" || other.holder?.lead_sid === "host")) other.state = "done";
    const request = st.items.find((item) => item.id === "codex-approval");
    Object.assign(request, { agent_id: "a-loops2-i", agent_name: "loops2-i", thread: [{ role: "you", text: "Draw option I boards for the Loops 2 page." }] });
  });
  const row = page.locator('.dk-items [data-desk-item="295"]');
  await expect(row).toHaveClass(/\bask\b/);
  await expect(statusOf(page, "295")).toHaveText("Asks you");
  expect(await groupOf(page, "295")).toBe("WAITING ON YOU");
  await expect(page.locator('[data-desk-request="codex-approval"]')).toHaveCount(0);
  await row.click();
  await page.locator('.dk-right [data-act="allow"][data-id="codex-approval"]').click();
  await cue.called("respond", { id: "codex-approval", decision: { behavior: "allow" } });
});
