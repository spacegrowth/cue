import { test, expect } from "./cue.mjs";

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
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
}

test("Desk surfaces a Codex request without an item tag and Allow clears it in the session too", async ({ cue, page }) => {
  await setup(cue, page);
  const request = page.locator('[data-desk-request="codex-approval"]');
  await expect(request).toContainText("host-lead");
  await expect(request).toContainText("printf approval-check");
  await page.locator(".dk-sessions").screenshot({ path: "/tmp/cue-desk-approval.png" });
  await request.locator('[data-act="allow"]').click();
  await cue.called("respond", { id: "codex-approval", decision: { behavior: "allow" } });
  await expect(request).toHaveCount(0);
  await page.locator('.dk-sessions [data-desk-chat-of="host"]').click();
  await page.locator('.dk-right [data-desk-session="host"]').click();
  await expect(page.locator('.active-pane [data-act="allow"][data-id="codex-approval"]')).toHaveCount(0);
  expect(await page.evaluate(() => state.items.some((it) => it.id === "codex-approval"))).toBe(false);
});

test("the worker chat on Desk has the live approval footer and Deny updates its request list", async ({ cue, page }) => {
  await setup(cue, page);
  await page.locator('[data-desk-request="codex-approval"] [data-desk-chat-of="host"]').click();
  await expect(page.locator(".dk-right .dk-ask")).toContainText("printf approval-check");
  await page.locator('.dk-right [data-act="deny"][data-id="codex-approval"]').click();
  await cue.called("respond", { id: "codex-approval", decision: { behavior: "deny" } });
  await expect(page.locator('[data-desk-request="codex-approval"]')).toHaveCount(0);
  await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
});

test("Allow from the session removes the same pending request from Desk", async ({ cue, page }) => {
  await setup(cue, page);
  await page.locator('[data-desk-request="codex-approval"] [data-desk-chat-of="host"]').click();
  await page.locator('.dk-right [data-desk-session="host"]').click();
  await page.locator('.active-pane [data-act="allow"][data-id="codex-approval"]').click();
  await cue.called("respond", { id: "codex-approval" });
  await page.locator('.switch-view [data-view="desk"]').click();
  await expect(page.locator('[data-desk-request="codex-approval"]')).toHaveCount(0);
  await expect(page.locator('.dk-right [data-id="codex-approval"]')).toHaveCount(0);
});

for (const choice of ["Allow", "No"]) {
  test(`Computer Use confirmation shows right-aligned Allow and No and ${choice} answers it from Desk`, async ({ cue, page }) => {
    await setup(cue, page);
    await cue.change((st) => {
      const request = st.items.find((it) => it.id === "codex-approval");
      request.kind = "input";
      request.tool_name = "cua_repl";
      request.tool_input = { server: "cua_repl", message: 'Allow Computer Use to use "Cue"?', form: { type: "object", properties: {} } };
    });
    const card = page.locator('[data-desk-request="codex-approval"]');
    await expect(card).toContainText('Allow Computer Use to use "Cue"?');
    await expect(card.locator(".row-btns button")).toHaveText(["No", "Allow"]);
    const panel = await card.boundingBox(), yes = await card.getByRole("button", { name: "Allow", exact: true }).boundingBox();
    expect(panel.x + panel.width - yes.x - yes.width).toBeLessThan(20);
    expect(yes.width).toBeLessThan(panel.width / 2);
    await card.locator('[data-desk-chat-of="host"]').click();
    const footer = page.locator(".dk-right .dk-ask");
    await expect(footer.locator(".row-btns button")).toHaveText(["No", "Allow"]);
    const bounds = await footer.locator(".act-card").boundingBox(), right = await footer.getByRole("button", { name: "Allow", exact: true }).boundingBox();
    expect(bounds.x + bounds.width - right.x - right.width).toBeLessThan(20);
    await page.screenshot({ path: `/tmp/cue-desk-computer-use-${choice.toLowerCase()}.png` });
    const source = choice === "Allow" ? card : footer;
    await source.getByRole("button", { name: choice, exact: true }).click();
    await cue.called("respond", { id: "codex-approval", decision: choice === "Allow" ? { behavior: "allow", answers: {} } : { behavior: "deny" } });
    await expect(card).toHaveCount(0);
    await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
  });
}
