// A pick says in the chats what it started (#917): the item you picked on keeps one line, "You picked A — #p17 builds
// it", its number opening the item that builds it; that item's chat opens with "From your pick of A on #d5", its
// hand-off card in Activity as any other. Light and dark.
import { test, expect, openDesk } from "./cue.mjs";

const PICKED = "Desk item #d5 (Mocks: the header): option A was picked, so it's done. #p17 builds it. What follows: #p17, which the Desk lead filed. Do no other work on #d5.";
const BRIEF = "Desk item #p17 (feature): Build: the header (option A)\nYou'll see: the header from option A\nDone when: it's in the app\nYou picked option A on #d5: https://example.com/canvas\nWhy you: it's free";

const setUp = (cue) => cue.change((st, [PICKED, BRIEF]) => {
  const d = st.desk.projects.find((p) => p.project === "cue"), at = Date.now() - 600000;
  d.items.push({ id: "p17", project: "cue", kind: "feature", title: "Build: the header (option A)", state: "working", created: at, updated: at, builds: "d5", from_pick: "A", holder: { kind: "session", sid: "site", name: "site-lead" }, from: "you" });
  st.sessions.find((x) => x.session_id === "host").thread = [
    { role: "you", text: PICKED, cue: true, kind: "note", at_ms: at, images: [] },
  ];
  st.sessions.find((x) => x.session_id === "site").thread = [
    { role: "you", text: BRIEF, cue: true, kind: "brief", at_ms: at + 60000, images: [] },
    { role: "agent", text: "On it.", at_ms: at + 120000 },
  ];
}, [PICKED, BRIEF]);

for (const scheme of ["light", "dark"]) {
  test(`the picked item's chat names what builds it, and that item's chat says where it came from (${scheme})`, async ({ page, cue }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await cue.open("desk");
    await cue.override("session_log_page", () => []);
    await cue.override("transcript_page", () => []);
    await setUp(cue);
    await openDesk(page);
    await page.locator('.dkn-items [data-desk-item="d5"]').first().click();
    const chat = page.locator('.dk-right [data-desk-chat="host"]');
    const line = chat.locator(".dk-picked");
    await expect(line).toHaveText("CueYou picked A — #p17 builds it");
    await expect(line).toBeVisible();
    const link = line.locator('[data-desk-hand="cue"][data-id="p17"]');
    expect(await link.evaluate((el) => getComputedStyle(el).color)).not.toBe(await line.evaluate((el) => getComputedStyle(el).color));
    expect(await link.getAttribute("title")).toBeNull();
    await link.click();
    const opened = page.locator('.dk-right [data-desk-chat="site"]');
    const from = opened.locator(".dk-picked");
    await expect(from).toHaveText("CueFrom your pick of A on #d5");
    // Its hand-off card isn't in the chat: Activity has it.
    await expect(opened.locator(".mg-hand")).toHaveCount(0);
    const acts = await page.evaluate(() => dkActivityOf("site").map((r) => r.html));
    expect(acts.filter((h) => h.includes("mg-hand"))).toHaveLength(1);
    // Its number goes back to the item you picked on.
    await from.locator('[data-id="d5"]').click();
    await expect(page.locator('.dk-right [data-desk-chat="host"] .dk-picked')).toBeVisible();
  });
}
