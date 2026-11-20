// A session whose turn is over while its tests, build, install or landing run in the background is at work (#556):
// it counts in the head's "N working", the Working group, the Dispatcher's Working list, and its row says what runs;
// its item reads as working, never waiting.
import { test, expect, openDesk } from "./cue.mjs";

for (const running of ["Running tests…", "Building…", "Landing…"]) {
  test(`a session ${running.toLowerCase()} in the background counts as working everywhere the Desk shows it (#556)`, async ({ page, cue }) => {
    await cue.open("desk");
    await openDesk(page);
    const sub = page.locator(".dk-strip .dk-sub");
    const count = async () => Number(((await sub.textContent()).match(/(\d+) working/) || [0, 0])[1]);
    const working = page.locator(".dk-sessions .dk-gh", { hasText: "Working" }).locator(".n");
    const panel = page.locator(".dk-right .dk-psec", { hasText: "Working ·" });
    const panelCount = async () => Number(((await panel.textContent()).match(/Working · (\d+)/) || [0, 0])[1]);
    const before = await count(), beforePanel = await panelCount();
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
    await expect.poll(panelCount).toBe(beforePanel + 1);
    await expect(page.locator('.dk-row[data-desk-item="d556"], [data-desk-item="d556"]').first()).not.toContainText("waiting");
    // Its run over and its turn ended: idle again, out of every count.
    await cue.change((st) => { const s = st.sessions.find((x) => x.session_id === "s-run"); s.running = ""; });
    await expect(st.locator("b")).toHaveText("idle");
    await expect.poll(count).toBe(before);
    await expect.poll(panelCount).toBe(beforePanel);
  });
}
