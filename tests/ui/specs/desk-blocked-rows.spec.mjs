// Rows under Blocked on you (#802): the band's pink stays under the pointer and when the row is open; an outline in
// the band's red marks them instead of a fill. Other bands keep their fills.
import { test, expect, openDesk } from "./cue.mjs";

const SHOTS = process.env.CUE_802_SHOTS;

const T = 1_800_000_000_000;

const open = async (page, cue, size) => {
  await page.setViewportSize(size);
  await cue.open("desk");
  // Item 227's session asks a step held for your OK (a push to main): its row is under Blocked on you (#749).
  await cue.change((st, T) => {
    st.sessions.find((s) => s.session_id === "host").state = "deciding";
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const item of d.items.filter((i) => i.holder?.sid === "host")) { item.state = "working"; item.now_on = false; item.ask = null; }
    Object.assign(d.items.find((item) => item.holder?.sid === "host"), { id: "227", kind: "feature", title: "Give the Desk its own icon" });
    st.items.push({ id: "perm", session_id: "host", project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command: "git push origin main" }, one_way: true, suggestions: [], created_ms: T, thread: [{ role: "you", text: "Desk item #227 (feature): Give the Desk its own icon" }] });
  }, T);
  await openDesk(page);
  await expect(page.locator(".dk-board.dk-c")).toHaveCount(1);
};

const look = (loc) => loc.evaluate((el) => { const s = getComputedStyle(el); return { bg: s.backgroundColor, shadow: s.boxShadow, color: s.color }; });
const band = (page) => page.locator(".dkn-card .dkn-b-blocked > .dkn-bb").evaluate((el) => getComputedStyle(el).backgroundColor);

for (const scheme of ["light", "dark"]) {
  for (const [name, size] of [["laptop", { width: 1440, height: 900 }], ["smallest", { width: 900, height: 600 }]]) {
    test(`Blocked on you rows keep the band's pink on hover and open (${scheme}, ${name})`, async ({ page, cue }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await open(page, cue, size);
      const row = page.locator('.dkn-card .dkn-b-blocked .dkn-row[data-desk-item="227"]');
      await expect(row).toHaveCount(1);
      const pink = await band(page);
      const deny = await page.evaluate(() => { const d = document.createElement("i"); d.style.color = "var(--deny)"; document.body.append(d); const c = getComputedStyle(d).color; d.remove(); return c; });
      const rest = await look(row);
      expect(rest.bg).toBe("rgba(0, 0, 0, 0)");
      expect(rest.shadow).toBe("none");
      // Pointed at: no fill, a 1px outline in the band's red; the text keeps its colour.
      await row.hover();
      const hov = await look(row);
      expect(hov.bg).toBe("rgba(0, 0, 0, 0)");
      expect(hov.shadow).toBe(`${deny} 0px 0px 0px 1px inset`);
      expect(hov.color).toBe(rest.color);
      if (SHOTS) await page.locator(".dkn-items").screenshot({ path: `${SHOTS}/${scheme}-${name}-hover.png` });
      // Opened: still no fill, a 2px outline; the band's pink shows through.
      await row.click();
      await expect(row).toHaveClass(/\bon\b/);
      await page.mouse.move(0, 0);
      const on = await look(row);
      expect(on.bg).toBe("rgba(0, 0, 0, 0)");
      expect(on.shadow).toBe(`${deny} 0px 0px 0px 2px inset`);
      expect(on.color).toBe(rest.color);
      expect(await band(page)).toBe(pink);
      if (SHOTS) await page.locator(".dkn-items").screenshot({ path: `${SHOTS}/${scheme}-${name}-open.png` });
      // A row in another band still fills when pointed at.
      const other = page.locator(".dkn-card .dkn-b-working .dkn-row[data-desk-item]").first();
      await other.hover();
      expect((await look(other)).bg).not.toBe("rgba(0, 0, 0, 0)");
    });
  }
}
