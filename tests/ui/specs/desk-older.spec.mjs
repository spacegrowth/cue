// Fewer sessions on show (#363): the Desk's lists show the sessions with work (the hub's `with_work`); the rest fold
// into one "N older" line, shut by default, that opens in place.
import { test, expect } from "./cue.mjs";

const openDesk = async (page) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
  await page.locator(".dk-items .dk-it").first().waitFor();
};

/** Two idle sessions on the Desk, "busy" with work (an item) and "spare" with none. */
const twoIdle = async (cue) => {
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site");
    for (const sid of ["busy", "spare"]) st.sessions.push({ ...site, session_id: sid, project: "cue", name: sid, state: "idle", thread: [] });
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.members.push("busy", "spare");
    d.with_work = [d.lead.sid, "host", "busy"];
  });
};

test("idle sessions with no recent item fold into one cold line at the end of Idle, shut until you open it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await twoIdle(cue);
  const side = page.locator(".dk-sessions");
  await expect(side.locator('[data-desk-chat-of="busy"]')).toHaveCount(1);
  await expect(side.locator('[data-desk-chat-of="spare"]')).toHaveCount(0);
  const fold = side.locator('[data-desk-older="side:cue"]');
  await expect(fold).toHaveText("1 cold · show");
  // One Desk list (#428): Working, then Idle, the cold line last; the counts add up to the strip's session count.
  const heads = await side.locator(".dk-scroll > .dk-gh").evaluateAll((els) => els.map((el) => el.textContent.trim().replace(/\s+/g, " ")));
  expect(heads).toEqual(["Working 1", "Idle 2"]);
  await expect(side.locator(".dk-scroll > *").last()).toHaveAttribute("data-desk-older", "side:cue");
  await expect(side.locator(".dk-strip .dk-sub")).toHaveText(/^3 sessions/);
  await expect(side.locator(".dk-scroll")).not.toContainText("On the Desk");
  await expect(side.locator(".dk-scroll [title]")).toHaveCount(0);
  await expect(fold).toHaveAttribute("aria-expanded", "false");
  await fold.click();
  await expect(side.locator('[data-desk-chat-of="spare"]')).toHaveCount(1);
  await expect(fold).toHaveText("1 cold · hide");
});

test("Give to… lists the sessions with work, the rest under one older line", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await twoIdle(cue);
  await page.locator('[data-desk-item="d6"] [data-desk-give]').scrollIntoViewIfNeeded();
  await page.locator('[data-desk-item="d6"] [data-desk-give]').click();
  const menu = page.locator('[data-desk-item="d6"] .dk-givemenu');
  await expect(menu.locator('[data-desk-give-to="busy"]')).toHaveCount(1);
  await expect(menu.locator('[data-desk-give-to="spare"]')).toHaveCount(0);
  const older = menu.locator('[data-desk-older="give:d6"]');
  await expect(older).toHaveText(/older · show/);
  await older.click();
  await expect(menu.locator('[data-desk-give-to="spare"]')).toHaveCount(1);
});
