// A session whose turn is over while its tests, build, install or landing run in the background is at work (#556):
// it counts in the head's "N working", the Working group, the sessions column's "N working", and its row says what runs;
// its item reads as working, never waiting.
import { test, expect, openDesk, deskAllSessions } from "./cue.mjs";
import { groupOf } from "./desk.mjs";

for (const running of ["Running tests…", "Building…", "Landing…"]) {
  test(`a session ${running.toLowerCase()} in the background counts as working everywhere the Desk shows it (#556)`, async ({ page, cue }) => {
    await page.setViewportSize({ width: 1800, height: 900 });
    await cue.open("desk");
    await openDesk(page);
    // The sessions column's head: "N working" (desk/c.js deskCSessions).
    const panel = page.locator(".dkn-side .dkn-cnt");
    const panelCount = async () => Number(((await panel.textContent()).match(/(\d+) working/) || [0, 0])[1]);
    await expect(panel).toBeVisible();
    const beforePanel = await panelCount();
    // The sessions list (All N sessions), in the lead's chat's place (B2, #666).
    await deskAllSessions(page);
    const sub = page.locator(".dk-strip .dk-sub");
    const count = async () => Number(((await sub.textContent()).match(/(\d+) working/) || [0, 0])[1]);
    const working = page.locator(".dk-sessions .dk-gh", { hasText: "Working" }).locator(".n");
    const before = await count();
    // Its turn over (waiting at its prompt) while its run goes on, on an item of this Desk.
    await cue.change((st, running) => {
      st.sessions.push({ session_id: "s-run", name: "cue-556", project: "cue", cwd: "/Users/me/cue", state: "waiting", running, since_ms: Date.now(), thread: [] });
      const d = st.desk.projects.find((x) => x.project === "cue");
      d.members.push("s-run");
      const base = d.items.find((i) => i.state === "working") || d.items[0];
      d.items.push({ ...base, id: "d556", title: "Header", state: "working", waiting: "", stopped: false, ask: null, latest: running, holder: { kind: "session", sid: "s-run", name: "cue-556" } });
    }, running);
    await expect.poll(count).toBe(before + 1);
    await expect(working).toHaveText(String(before + 1));
    const st = page.locator('.dk-sessions [data-desk-chat-of="s-run"] .dk-st').first();
    await expect(st.locator("b")).toHaveText(`working · ${running.charAt(0).toLowerCase()}${running.slice(1)}`);
    await expect(st.locator("i")).not.toHaveClass(/idle/);
    // Back to the lead's chat: the sessions column counts it; its item, in the Items list, is at work, not waiting.
    await page.locator(".dk-sessions .dk-back").first().click();
    await expect.poll(panelCount).toBe(beforePanel + 1);
    await expect(page.locator('.dk-items [data-desk-item="d556"]')).toHaveCount(1);
    expect(await groupOf(page, "d556")).toBe("WORKING");
    await expect(page.locator('.dk-items [data-desk-item="d556"]')).not.toContainText(/waiting/i);
    // Its run over and its turn ended: idle again, out of every count.
    await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "s-run"); s.running = ""; });
    await expect.poll(panelCount).toBe(beforePanel);
    await deskAllSessions(page);
    await expect(st.locator("b")).toHaveText("idle");
    await expect.poll(count).toBe(before);
  });
}
