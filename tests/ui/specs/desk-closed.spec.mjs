// Closed sessions in the Desk's sessions column (#834): ones Cue's record says no process runs any more, holding no
// open item, fold under a CLOSED heading at the foot, shut till you open it; their rows muted, saying closed.
import { test, expect, openDesk } from "./cue.mjs";

/** "gone" and "old" ended (no process), "kept" ended but holds an open item, "awake" still runs. */
const four = (cue) => cue.change((st) => {
  const site = st.sessions.find((x) => x.session_id === "site"), t = Date.now();
  for (const [sid, pid, m] of [["gone", null, 5], ["old", null, 300], ["kept", null, 6], ["awake", 9000003, 7]]) st.sessions.push({ ...site, session_id: sid, project: "cue", name: sid, state: "waiting", agent_pid: pid, since_ms: t - m * 60000, thread: [] });
  const d = st.desk.projects.find((x) => x.project === "cue");
  d.members.push("gone", "old", "kept", "awake");
  d.items.push({ id: "d834", project: "cue", kind: "bug", title: "Held by kept", state: "queued", holder: { kind: "session", sid: "kept", name: "kept" }, updated: Math.floor(t / 1000) });
});
const rows = (side) => side.locator(".dkn-list > *").evaluateAll((els) => els.map((el) => el.dataset.deskChatOf || `# ${el.textContent.replace(/\s+/g, " ").trim()}`));

for (const scheme of ["light", "dark"]) {
  test(`ended sessions fold under CLOSED at the foot of the sessions column; opened, muted and saying closed; one that wakes moves back (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1600, height: 1000 });
    await cue.open("desk");
    await openDesk(page);
    await four(cue);
    const side = page.locator(".dkn-side"), head = side.locator('[data-desk-older="closed:cue"]');
    await expect(head).toHaveAttribute("aria-expanded", "false");
    await expect(head).toHaveText(/CLOSED\s*· 2/);
    let r = await rows(side);
    expect(r.at(-1)).toMatch(/^# .*CLOSED/);
    expect(r).toContain("kept");
    expect(r).toContain("awake");
    expect(r).not.toContain("gone");
    await head.click({ position: { x: 150, y: 10 } });
    await expect(head).toHaveAttribute("aria-expanded", "true");
    r = await rows(side);
    expect(r.slice(-3)).toEqual([r.at(-3), "gone", "old"]);
    const gone = side.locator('[data-desk-chat-of="gone"]');
    await expect(gone).toHaveClass(/closed/);
    await expect(gone.locator(".dkn-s")).toHaveText("closed");
    await expect(side.locator("[title]")).toHaveCount(0);
    if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-c-closed-${scheme}.png`, clip: await side.boundingBox() });
    // The heading folds shut again.
    await head.click();
    await expect(head).toHaveAttribute("aria-expanded", "false");
    await expect(gone).toHaveCount(0);
    // It wakes: back among the rest on the next update, no longer muted.
    await cue.change((st) => { st.sessions.find((x) => x.session_id === "gone").agent_pid = 9000004; });
    await expect(head).toHaveText(/CLOSED\s*· 1/);
    await expect(side.locator('[data-desk-chat-of="gone"]')).not.toHaveClass(/closed/);
  });
}

test("in a laptop's narrow column the CLOSED heading shows its arrow and word, still folding (#834)", async ({ page, cue }) => {
  await page.setViewportSize({ width: 900, height: 600 });
  await cue.open("desk");
  await openDesk(page);
  await four(cue);
  const head = page.locator('.dkn-side [data-desk-older="closed:cue"]');
  await head.scrollIntoViewIfNeeded();
  await expect(head).toBeVisible();
  await expect(head.locator("b")).toBeVisible();
  await expect(head.locator(".dkn-fa")).toBeVisible();
  await head.click();
  await expect(head).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator('.dkn-side [data-desk-chat-of="gone"]')).toHaveCount(1);
  if (process.env.SHOT_DIR) await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-c-closed-small.png` });
});
