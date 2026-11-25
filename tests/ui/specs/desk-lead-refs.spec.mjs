// Item numbers and session names in the lead chat (#828): "#d2" or "Bug #d2" opens that item; a session's name opens
// its chat. A number no item has, a word no session is named, and anything in code stay plain text.
import { test, expect, openDesk } from "./cue.mjs";

const push = (page, text) => page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, text);

test("items and sessions the lead names are links that open them; unknown ones and code stay plain", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const other = await page.evaluate(() => { const s = state.sessions.find((x) => x.session_id !== "lead" && /-/.test(nameOf(x.session_id, x.project))); return s && { sid: s.session_id, name: nameOf(s.session_id, s.project) }; });
  expect(other).toBeTruthy();
  await push(page, [
    `Bug #d2 is with ${other.name}, and #d2 again; #9999 isn't an item, nor is not-a-session.`,
    "",
    `Run \`${other.name} #d2\` there.`,
    "",
    "```",
    `#d2 ${other.name}`,
    "```",
  ].join("\n"));
  const msg = page.locator(".dk-chat .cv-agent").last();
  const items = msg.locator("a[data-desk-chk-item]");
  await expect(items).toHaveText([/^Bug #d2$/i, "#d2"]);
  await expect(msg.locator("a[data-desk-session]")).toHaveText([other.name]);
  await expect(msg.locator("code a, pre a")).toHaveCount(0);
  await expect(msg.locator("a", { hasText: "9999" })).toHaveCount(0);
  await expect(msg.locator("a", { hasText: "not-a-session" })).toHaveCount(0);
  await expect(msg.locator("[title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) await msg.screenshot({ path: `${process.env.SHOT_DIR}/lead-refs.png` });
  await items.first().click();
  await expect(page.locator('.dkn-card .dkn-row.on[data-desk-item="d2"]')).toHaveCount(1);
  await page.evaluate(() => { deskTab = "lead"; deskShow(null); });
  await page.locator(`.dk-chat .cv-agent a[data-desk-session="${other.sid}"]`).last().click();
  await expect.poll(() => page.evaluate(() => active?.sid)).toBe(other.sid);
});
