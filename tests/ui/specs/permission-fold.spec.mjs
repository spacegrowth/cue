// A permission for a long script (#611): its card shows the first three lines, cut at whole lines, with more to open
// the rest; Deny and Allow in view without scrolling. On an item's page the script showed whole above its card (the
// item's question block repeated the ask's text, the full command); now only the folded card has it. Light and dark.
import { test, expect, openDesk } from "./cue.mjs";

const SCRIPT = Array.from({ length: 30 }, (_, n) => `echo "step ${n}" && ls -la build/out/part-${n} | grep -v tmp`).join("\n");

/** The Desk with item #227 held by a hosted Claude session that asks to run `cmd`. */
async function setup(cue, page, cmd) {
  await cue.open("desk");
  await cue.change((st, command) => {
    const worker = st.sessions.find((s) => s.session_id === "host");
    worker.harness = "claude"; worker.hosted = true; worker.state = "deciding";
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const item of d.items.filter((i) => i.holder?.sid === "host")) { item.state = "working"; item.now_on = false; item.ask = null; }
    const it = d.items.find((item) => item.holder?.sid === "host");
    // As the hub shows it (desk ask_view): the ask's text is the whole command.
    Object.assign(it, { id: "227", kind: "feature", title: "Give the Desk its own icon", state: "needs_you", ask: { item_id: "perm", sid: "host", text: command, choices: [], kind: "permission" } });
    st.items.push({ id: "perm", session_id: "host", harness: "claude", hosted: true, project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command }, suggestions: [], created_ms: Date.now(), thread: [{ role: "you", text: "Desk item #227 (feature): Give the Desk its own icon" }] });
  }, cmd);
  await openDesk(page);
}

/** The folded command in `card`: its lines shown, whether more is under it, and lines its clip edge cuts through. */
const fold = (page, card) => page.locator(`${card} .req-cut pre`).evaluate((el) => {
  const cs = getComputedStyle(el), r = el.getBoundingClientRect();
  const edge = r.bottom - parseFloat(cs.borderBottomWidth), lh = parseFloat(cs.lineHeight);
  const rects = [];
  const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  for (let n; (n = w.nextNode());) { const rg = document.createRange(); rg.selectNodeContents(n); rects.push(...rg.getClientRects()); }
  const shown = new Set(rects.filter((q) => q.height && q.bottom <= edge + 1).map((q) => Math.round(q.top))).size;
  return { shown, more: el.scrollHeight > el.clientHeight + lh, cut: rects.filter((q) => q.height && q.top < edge - 1 && q.bottom > edge + 1).length };
});

/** Whether `sel` is fully on screen, and inside every box above it that scrolls or clips. */
const inView = (page, sel) => page.locator(sel).evaluate((el) => {
  const r = el.getBoundingClientRect();
  if (r.top < 0 || r.bottom > innerHeight || r.left < 0 || r.right > innerWidth) return false;
  for (let p = el.parentElement; p; p = p.parentElement) {
    if (!/auto|scroll|hidden/.test(getComputedStyle(p).overflowY)) continue;
    const b = p.getBoundingClientRect();
    if (r.top < b.top - 1 || r.bottom > b.bottom + 1) return false;
  }
  return true;
});

for (const scheme of ["light", "dark"]) {
  test(`an item's permission for a long script: three whole lines with more, Deny and Allow in view (${scheme}) (#611)`, async ({ cue, page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await setup(cue, page, SCRIPT);
    await page.locator('.dk-items [data-desk-item="227"]').click();
    const card = ".dk-right .act-card";
    await expect(page.locator(card)).toBeVisible();
    // The script is in the card only: nothing above it repeats it.
    await expect(page.locator(".dk-right").getByText("step 12", { exact: false })).toHaveCount(1);
    await expect(page.locator(".dk-right .dk-q")).toHaveCount(0);
    // Folded: its first three lines, whole, with more under them.
    await expect.poll(() => fold(page, card)).toEqual({ shown: 3, more: true, cut: 0 });
    expect(await inView(page, `${card} [data-act="deny"]`)).toBe(true);
    expect(await inView(page, `${card} [data-act="allow"]`)).toBe(true);
    // No hover text on the card.
    await expect(page.locator(`${card} [title]`)).toHaveCount(0);
    // more opens the rest in place; less folds it back.
    await page.locator(`${card} [data-req-more]`).click();
    await expect(page.locator(`${card} .req-cut.open`)).toHaveCount(1);
    await page.locator(`${card} [data-req-more]`).click();
    await expect.poll(() => fold(page, card)).toEqual({ shown: 3, more: true, cut: 0 });
  });
}

test("once denied, the item's page shows the command folded with what you answered (#611)", async ({ cue, page }) => {
  await setup(cue, page, SCRIPT);
  await page.locator('.dk-items [data-desk-item="227"]').click();
  await page.locator('.dk-right .act-card [data-act="deny"]').click();
  // The request is gone and its turn stopped there (its session idle): its page says what you answered, the command
  // folded, not the whole script.
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.id !== "perm");
    st.sessions.find((s) => s.session_id === "host").state = "idle";
  });
  const q = page.locator(".dk-right .dk-q");
  await expect(q).toContainText("You answered: Declined");
  await expect.poll(() => fold(page, ".dk-right .dk-q")).toEqual({ shown: 3, more: true, cut: 0 });
});

test("a session's chat: a long script's card is three whole lines with more, Allow in view (#611)", async ({ cue, page }) => {
  await cue.open("world-one-ask");
  await cue.change((st, c) => { st.items[0].tool_input.command = c; }, SCRIPT);
  await expect.poll(() => fold(page, ".act-card")).toEqual({ shown: 3, more: true, cut: 0 });
  expect(await inView(page, '.act-card [data-act="allow"]')).toBe(true);
  expect(await inView(page, '.act-card [data-act="deny"]')).toBe(true);
});
